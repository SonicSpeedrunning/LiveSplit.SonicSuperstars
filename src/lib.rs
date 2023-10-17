#![no_std]
#![feature(type_alias_impl_trait, const_async_blocks)]
#![warn(
    clippy::complexity,
    clippy::correctness,
    clippy::perf,
    clippy::style,
    clippy::undocumented_unsafe_blocks,
    rust_2018_idioms
)]

use asr::{
    future::{next_tick, retry},
    game_engine::unity::{il2cpp::{Module, Version, UnityPointer, Image}, SceneManager, get_scene_name},
    time::Duration,
    timer::{self, TimerState},
    watcher::Watcher,
    Process,
};

asr::panic_handler!();
asr::async_main!(nightly);

const PROCESS_NAMES: &[&str] = &["SonicSuperstars.exe"];

async fn main() {
    let settings = Settings::register();

    loop {
        // Hook to the target process
        let process = retry(|| PROCESS_NAMES.iter().find_map(|name| Process::attach(name))).await;

        process
            .until_closes(async {
                // Once the target has been found and attached to, set up some default watchers
                let mut watchers = Watchers::default();

                // Perform memory scanning to look for the addresses we need
                let memory = retry(|| Memory::init(&process)).await;

                loop {
                    // Splitting logic. Adapted from OG LiveSplit:
                    // Order of execution
                    // 1. update() will always be run first. There are no conditions on the execution of this action.
                    // 2. If the timer is currently either running or paused, then the isLoading, gameTime, and reset actions will be run.
                    // 3. If reset does not return true, then the split action will be run.
                    // 4. If the timer is currently not running (and not paused), then the start action will be run.
                    update_loop(&process, &memory, &mut watchers);

                    let timer_state = timer::state();
                    if timer_state == TimerState::Running || timer_state == TimerState::Paused {
                        if let Some(is_loading) = is_loading(&watchers, &settings) {
                            if is_loading {
                                timer::pause_game_time()
                            } else {
                                timer::resume_game_time()
                            }
                        }

                        if let Some(game_time) = game_time(&watchers, &settings, &memory) {
                            timer::set_game_time(game_time)
                        }

                        if reset(&watchers, &settings) {
                            timer::reset()
                        } else if split(&watchers, &settings) {
                            timer::split()
                        }
                    }

                    if timer::state() == TimerState::NotRunning && start(&watchers, &settings) {
                        timer::start();
                        timer::pause_game_time();

                        if let Some(is_loading) = is_loading(&watchers, &settings) {
                            if is_loading {
                                timer::pause_game_time()
                            } else {
                                timer::resume_game_time()
                            }
                        }
                    }

                    next_tick().await;
                }
            })
            .await;
    }
}

#[derive(asr::user_settings::Settings)]
struct Settings {
    #[default = true]
    /// => Enable auto start
    start: bool,
            // settings to be added for each level
}

#[derive(Default)]
struct Watchers {
    is_loading: Watcher<bool>,
}

struct Memory {
    il2cpp_module: Module,
    game_assembly: Image,
    scene_manager: SceneManager,
}

impl Memory {
    fn init(game: &Process) -> Option<Self> {
        let il2cpp_module = Module::attach(game, Version::V2020)?;
        let game_assembly = il2cpp_module.get_default_image(game)?;
        let scene_manager = SceneManager::attach(game)?;
        
        //let is_loading = UnityPointer::new("SceneManager", 1, &["s_sInstance", "m_bProcessing"]);

        Some(Self {
            il2cpp_module,
            game_assembly,
            scene_manager,
        })
    }
}

fn update_loop(game: &Process, addresses: &Memory, watchers: &mut Watchers) {
    watchers.is_loading.update_infallible({
        let scene_path = addresses.scene_manager.get_current_scene_path::<128>(game);

        let scene_name = match &scene_path {
            Ok(x) => Some(get_scene_name(x)),
            _ => None,
        };

        match scene_name {
            Some(b"ReleaseScene") | Some(b"GameMain") => true,
            Some(_) => false,
            None => match &watchers.is_loading.pair {
                Some(x) => x.current,
                _ => false,
            }
        }
    });
}

fn start(watchers: &Watchers, settings: &Settings) -> bool {
    false
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    false
}

fn reset(_watchers: &Watchers, _settings: &Settings) -> bool {
    false
}

fn is_loading(watchers: &Watchers, _settings: &Settings) -> Option<bool> {
    Some(watchers.is_loading.pair?.current)
}

fn game_time(_watchers: &Watchers, _settings: &Settings, _addresses: &Memory) -> Option<Duration> {
    None
}
