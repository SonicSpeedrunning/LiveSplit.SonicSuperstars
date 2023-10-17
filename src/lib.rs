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
    game_engine::unity::{
        get_scene_name,
        il2cpp::{Image, Module, UnityPointer, Version},
        SceneManager,
    },
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
    #[default = true]
    /// Split after completion of each level
    split: bool,
}

#[derive(Default)]
struct Watchers {
    start_trigger: Watcher<bool>,
    level_id: Watcher<u32>,
    is_loading: Watcher<bool>,
    goal_ring_flag: Watcher<bool>,
}

struct Memory {
    il2cpp_module: Module,
    game_assembly: Image,
    scene_manager: SceneManager,
    is_loading: UnityPointer<1>,
    current_scene_controller: UnityPointer<2>,
    next_scene_name: UnityPointer<3>,
}

impl Memory {
    fn init(game: &Process) -> Option<Self> {
        let il2cpp_module = Module::attach(game, Version::V2020)?;
        let game_assembly = il2cpp_module.get_default_image(game)?;
        let scene_manager = SceneManager::attach(game)?;

        let is_loading =
            UnityPointer::new("Scene_Manager", 0, &["<IsTransitionPlay>k__BackingField"]);

        let current_scene_controller = UnityPointer::new(
            "Scene_Manager",
            2,
            &["s_Instance", "<CurrentSceneController>k__BackingField"],
        );

        let next_scene_name = UnityPointer::new(
            "Scene_Manager",
            2,
            &["s_Instance", "<NextSceneName>k__BackingField"],
        );

        Some(Self {
            il2cpp_module,
            game_assembly,
            scene_manager,
            is_loading,
            current_scene_controller,
            next_scene_name,
        })
    }
}

fn update_loop(game: &Process, addresses: &Memory, watchers: &mut Watchers) {
    watchers.start_trigger.update_infallible(
        if let Ok(scene_path) = addresses.scene_manager.get_current_scene_path::<128>(game) {
            let scene_name = get_scene_name(&scene_path);
            if scene_name == b"MainMenu" {
                let next_scene = game
                    .read_pointer_path64::<[u16; 10]>(
                        addresses
                            .next_scene_name
                            .deref_offsets(game, &addresses.il2cpp_module, &addresses.game_assembly)
                            .unwrap_or_default(),
                        &[0, 0x14],
                    )
                    .unwrap_or_default()
                    .map(|val| val as u8);
                &next_scene == b"MovieScene"
            } else {
                false
            }
        } else {
            false
        },
    );

    watchers.level_id.update_infallible(
        game.read_pointer_path64(
            addresses
                .current_scene_controller
                .deref_offsets(game, &addresses.il2cpp_module, &addresses.game_assembly)
                .unwrap_or_default(),
            &[0, 0x40, 0x10],
        )
        .unwrap_or_default(),
    );

    watchers.is_loading.update_infallible(
        addresses
            .is_loading
            .deref::<bool>(game, &addresses.il2cpp_module, &addresses.game_assembly)
            .unwrap_or_default(),
    );

    watchers.goal_ring_flag.update_infallible(
        game.read_pointer_path64(
            addresses
                .current_scene_controller
                .deref_offsets(game, &addresses.il2cpp_module, &addresses.game_assembly)
                .unwrap_or_default(),
            &[0, 0xF3],
        )
        .unwrap_or_default(),
    );
}

fn start(watchers: &Watchers, settings: &Settings) -> bool {
    settings.start
        && watchers
            .start_trigger
            .pair
            .is_some_and(|val| val.changed_to(&true))
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    settings.split
        && watchers
            .goal_ring_flag
            .pair
            .is_some_and(|val| val.changed_to(&false))
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
