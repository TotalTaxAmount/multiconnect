pub mod pairing;

use std::{
  any::{Any, TypeId},
  collections::HashMap,
  sync::Arc,
  time::Duration,
};

use async_trait::async_trait;
use multiconnect_protocol::Packet;
use tauri::{AppHandle, Wry};
use tokio::{
  sync::{broadcast, mpsc, Mutex},
  time::interval,
};

use crate::daemon::SharedDaemon;

#[macro_export]
macro_rules! with_manager_module {
  ($manager:expr, $t:ty, |$mod_var:ident, $ctx_var:ident| $body:block) => {{
    let module_entry = $manager.get::<$t>().ok_or("Module not found")?;
    let mut $mod_var = module_entry.lock().await;
    let $mod_var = $mod_var.as_any_mut().downcast_mut::<$t>().ok_or("Downcast failed")?;

    let ctx_lock = $manager.get_ctx().await;
    let mut $ctx_var = ctx_lock.lock().await;

    $body
  }};
}

#[macro_export]
macro_rules! with_ctx {
  ($manager:expr, |$ctx_var:ident| $body:block) => {{
    let ctx_lock = $manager.get_ctx().await;
    let mut $ctx_var = ctx_lock.lock().await;

    $body
  }};
}

pub struct FrontendCtx {
  app: AppHandle<Wry>,
  packet_tx: mpsc::Sender<Packet>,
}

impl FrontendCtx {
  pub fn new(app: AppHandle<Wry>, packet_tx: mpsc::Sender<Packet>) -> Self {
    Self { app, packet_tx }
  }

  pub async fn send_packet(&self, packet: Packet) {
    let _ = self.packet_tx.send(packet).await;
  }
}

#[async_trait]
pub trait FrontendModule: Send + Sync + Any {
  async fn init(&mut self, ctx: Arc<Mutex<FrontendCtx>>);
  async fn on_packet(&mut self, packet: Packet, ctx: &mut FrontendCtx);

  fn as_any_mut(&mut self) -> &mut dyn Any;
  fn as_any(&self) -> &dyn Any;
}

pub struct FrontendModuleManager {
  modules: HashMap<TypeId, Arc<Mutex<dyn FrontendModule>>>,
  ctx: Arc<Mutex<FrontendCtx>>,
  recv_packet_stream: broadcast::Receiver<Packet>,
}

impl FrontendModuleManager {
  pub fn new(daemon: SharedDaemon, app: AppHandle<Wry>) -> Self {
    Self {
      modules: HashMap::new(),
      ctx: Arc::new(Mutex::new(FrontendCtx::new(app, daemon.sending_stream()))),
      recv_packet_stream: daemon.packet_stream(),
    }
  }

  pub fn register<T: FrontendModule>(&mut self, module: T) {
    let type_id = module.as_any().type_id();
    self.modules.insert(type_id, Arc::new(Mutex::new(module)));
  }

  pub fn get<T: FrontendModule + 'static>(&self) -> Option<&Arc<Mutex<dyn FrontendModule + 'static>>> {
    let id = TypeId::of::<T>();
    self.modules.get(&id)
  }

  pub async fn get_ctx(&self) -> Arc<Mutex<FrontendCtx>> {
    self.ctx.clone()
  }

  pub async fn init(&self) {
    for module in self.modules.values() {
      module.lock().await.init(self.ctx.clone()).await;
    }

    let modules = self.modules.clone();
    let mut ch = self.recv_packet_stream.resubscribe();
    let ctx = self.ctx.clone();

    tokio::spawn(async move {
      loop {
        tokio::select! {
          packet = ch.recv() => if let Ok(packet) = packet {
            let mut ctx = ctx.lock().await;
            for module in modules.values() {
              module.lock().await.on_packet(packet.clone(), &mut ctx).await;
            }
          },
        }
      }
    });
  }
}
