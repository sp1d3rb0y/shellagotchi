//! The daemon main loop: loads (or creates) the pet, starts the IPC
//! server, and runs a periodic tick timer until SIGTERM/SIGINT, saving
//! the pet on shutdown. This is the entry point for `shellagotchi
//! daemon`.

use std::sync::Mutex;

use rand::seq::IndexedRandom;

use crate::clock::{Clock, SystemClock};
use crate::daemon::ipc::server::ServerState;
use crate::pet::state::{PetState, Species};

/// Runs the daemon until it receives SIGINT or SIGTERM, at which point it
/// persists the current pet state and returns.
pub async fn run() -> anyhow::Result<()> {
    crate::paths::ensure_dirs_exist()?;
    let cfg = crate::config::load()?;
    let state_path = crate::paths::state_file_path();

    let pet = crate::daemon::persist::load(&state_path)?.unwrap_or_else(|| {
        let now = SystemClock.now();
        PetState::newborn(cfg.pet_name.clone(), pick_random_species(), now)
    });

    let server_state = std::sync::Arc::new(ServerState {
        pet: Mutex::new(pet),
        config: cfg.clone(),
        state_path: state_path.clone(),
    });

    let socket_path = crate::paths::socket_path();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    let server_handle = tokio::spawn({
        let server_state = server_state.clone();
        async move { crate::daemon::ipc::server::serve(&socket_path, server_state, shutdown_rx).await }
    });

    let mut tick_interval =
        tokio::time::interval(std::time::Duration::from_secs(cfg.tick_interval_secs));
    // The first tick fires immediately; skip it since the pet was just
    // caught up (or freshly created) above.
    tick_interval.tick().await;

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                let mut pet = server_state.pet.lock().unwrap();
                let now = SystemClock.now();
                let mut rng = rand::rng();
                crate::pet::engine::catch_up(&mut pet, now, &server_state.config, &mut rng);
                if let Err(err) = crate::daemon::persist::save(&pet, &server_state.state_path) {
                    tracing::warn!("failed to persist pet state during periodic tick: {err}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT, saving and exiting");
                break;
            }
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM, saving and exiting");
                break;
            }
        }
    }

    let _ = shutdown_tx.send(());
    let _ = server_handle.await;

    let pet = server_state.pet.lock().unwrap();
    crate::daemon::persist::save(&pet, &server_state.state_path)?;

    Ok(())
}

/// Picks a random species for a freshly-hatched pet.
fn pick_random_species() -> Species {
    const SPECIES: [Species; 4] = [Species::Blob, Species::Cat, Species::Dragon, Species::Ghost];
    *SPECIES
        .choose(&mut rand::rng())
        .expect("SPECIES is non-empty")
}
