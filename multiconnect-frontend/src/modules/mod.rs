pub mod file_transfer;
pub mod pairing;

use std::{
  any::{Any, TypeId},
  collections::HashMap,
  error::Error,
  sync::Arc,
};

use async_trait::async_trait;
use log::{debug, warn};
use multiconnect_core::Packet;
use tauri::{AppHandle, Wry};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::daemon::{DaemonEvent, SharedDaemon};

#[macro_export]
macro_rules! with_manager_module {
  ($manager:expr, $t:ty, |$mod_var:ident, $ctx_var:ident| $body:block) => {{
    let _module_entry = $manager.get::<$t>().ok_or("Module not found")?;
    let mut $mod_var = _module_entry.lock().await;
    let $mod_var = $mod_var.as_any_mut().downcast_mut::<$t>().ok_or("Downcast failed")?;

    let _ctx = $manager.get_ctx().await;
    let mut $ctx_var = _ctx.lock().await;

    $body
  }};
}

#[macro_export]
macro_rules! with_ctx {
  ($manager:expr, |$ctx_var:ident| $body:block) => {{
    let _ctx = $manager.get_ctx().await;
    let $ctx_var = _ctx.lock().await;

    $body
  }};
}

pub enum FrontendAction {
  SendPacket { packet: Packet },
  Reconnect,
}

pub struct FrontendCtx {
  app: AppHandle<Wry>,
  command_tx: mpsc::Sender<FrontendAction>,
}

impl FrontendCtx {
  pub fn new(app: AppHandle<Wry>, command_tx: mpsc::Sender<FrontendAction>) -> Self {
    Self { app, command_tx }
  }

  pub async fn do_action(&self, packet: FrontendAction) {
    let _ = self.command_tx.send(packet).await;
  }
}

#[async_trait]
pub trait FrontendModule: Send + Sync + Any {
  async fn init(&mut self, ctx: Arc<Mutex<FrontendCtx>>) -> Result<(), Box<dyn Error>>;
  async fn on_event(&mut self, event: DaemonEvent, ctx: &mut FrontendCtx) -> Result<(), Box<dyn Error>>;

  fn as_any_mut(&mut self) -> &mut dyn Any;
  fn as_any(&self) -> &dyn Any;
}

pub struct FrontendModuleManager {
  daemon: SharedDaemon,
  modules: HashMap<TypeId, Arc<Mutex<dyn FrontendModule>>>,
  ctx: Arc<Mutex<FrontendCtx>>,
  // event_tx: mpsc::Sender<FrontendAction>,
  command_rx: Option<mpsc::Receiver<FrontendAction>>,
}

impl FrontendModuleManager {
  pub fn new(daemon: SharedDaemon, app: AppHandle<Wry>) -> Self {
    // let (event_tx, event_rx) = mpsc::channel(1000);
    let (command_tx, command_rx) = mpsc::channel(1000);
    Self {
      daemon,
      modules: HashMap::new(),
      ctx: Arc::new(Mutex::new(FrontendCtx::new(app, command_tx))),
      // event_tx,
      command_rx: Some(command_rx),
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

  pub async fn init(&mut self) {
    for module in self.modules.values() {
      if let Err(e) = module.lock().await.init(self.ctx.clone()).await {
        warn!("Error in module (init): {}", e);
      }
    }

    let daemon = self.daemon.clone();
    let modules = self.modules.clone();
    let mut ch = self.daemon.event_channel();
    let command_channel = self.daemon.command_channel();
    let mut command_rx = self.command_rx.take().unwrap();
    let ctx = self.ctx.clone();

    tokio::spawn(async move {
      loop {
        tokio::select! {
          event = ch.recv() => if let Ok(event) = event {
            let mut ctx = ctx.lock().await;
            for module in modules.values() {
              if let Err(e) = module.lock().await.on_event(event.clone(), &mut ctx).await {
                warn!("Error in module (on_event): {}", e);
              }
            }
          },
          command = command_rx.recv() => if let Some(command) = command {
            match command {
                FrontendAction::SendPacket { packet } => {
                  if let Err(e) = command_channel.send(crate::daemon::DaemonCommand::SendPacket { packet }).await {
                    warn!("Channel error: {}", e);
                  };
                },
                FrontendAction::Reconnect => { daemon.reconnect(); },
            }
          }
        }
      }
    });
  }
}
