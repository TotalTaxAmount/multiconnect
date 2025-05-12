use std::{
  any::{Any, TypeId},
  collections::HashMap,
  sync::Arc,
  time::Duration,
};

use async_trait::async_trait;
use multiconnect_protocol::Packet;
use tauri::{AppHandle, Wry};
use tokio::{sync::Mutex, time::interval};

use crate::daemon::SharedDaemon;

#[macro_export]
macro_rules! get_manager_module {
  ($m:expr, $t:ty) => {{
    let tr = $m.get::<$t>().ok_or("Module not found")?;
    let guard = tr.lock().await;
    guard.as_any().downcast_ref::<$t>().ok_or("Downcast failed")
  }};
}

#[async_trait]
pub trait FrontendModule: Send + Sync + Any {
  async fn init(&mut self, app: AppHandle<Wry>);
  async fn on_packet(&mut self, packet: Packet, app: AppHandle<Wry>);
  async fn periodic(&mut self, app: AppHandle<Wry>);

  fn as_any(&self) -> &dyn Any;
}

pub struct FrontendModuleManager {
  modules: HashMap<TypeId, Arc<Mutex<dyn FrontendModule>>>,
  daemon: SharedDaemon,
}

impl FrontendModuleManager {
  pub fn new(daemon: SharedDaemon) -> Self {
    Self { modules: HashMap::new(), daemon }
  }

  pub fn register<T: FrontendModule>(&mut self, module: T) {
    let type_id = module.as_any().type_id();
    self.modules.insert(type_id, Arc::new(Mutex::new(module)));
  }

  // TODO: it would be nice to be able to downcast this here and return a type T but alas
  pub fn get<T: FrontendModule + 'static>(&self) -> Option<&Arc<Mutex<dyn FrontendModule + 'static>>> {
    let id = TypeId::of::<T>();
    self.modules.get(&id)
  }

  pub async fn init(&self, app: AppHandle<Wry>) {
    for module in self.modules.values() {
      module.lock().await.init(app.clone()).await;
    }

    let daemon = self.daemon.clone();
    let modules = self.modules.clone();
    let mut interval = interval(Duration::from_millis(20));

    tokio::spawn(async move {
      let mut stream = daemon.packet_stream();
      loop {
        tokio::select! {
          packet = stream.recv() => if let Ok(packet) = packet {
            for module in modules.values() {
              module.lock().await.on_packet(packet.clone(), app.clone()).await;
            }
          },
          _ = interval.tick() => {
            for module in modules.values() {
              module.lock().await.periodic(app.clone()).await;
            }
          }
        }
      }
    });
  }
}
