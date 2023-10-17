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
    deep_pointer::DeepPointer,
    future::{next_tick, retry},
    game_engine::unity::{
        get_scene_name,
        il2cpp::{Module, Version},
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
                let memory = Memory::init(&process).await;

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
    scene_manager: SceneManager,
    is_loading: DeepPointer<1>,
    level_id: DeepPointer<4>,
    next_scene_name: DeepPointer<3>,
    is_result_sequence: DeepPointer<3>,
}

impl Memory {
    async fn init(game: &Process) -> Self {
        let il2cpp_module = Module::wait_attach(game, Version::V2020).await;
        let game_assembly = il2cpp_module.wait_get_default_image(game).await;
        let scene_manager = SceneManager::wait_attach(game).await;

        let scene_manager_class = game_assembly.wait_get_class(game, &il2cpp_module, "Scene_Manager").await;
        let scene_manager_static = scene_manager_class.wait_get_static_table(game, &il2cpp_module).await;
        let scene_manager_is_in_transition = scene_manager_class.wait_get_field_offset(
            game,
            &il2cpp_module,
            "<IsTransitionPlay>k__BackingField",
        ).await as _;

        let scene_manager_parent = scene_manager_class
            .wait_get_parent(game, &il2cpp_module).await
            .wait_get_parent(game, &il2cpp_module).await;
        let scene_manager_parent_static =
            scene_manager_parent.wait_get_static_table(game, &il2cpp_module).await;
        let scene_manager_parent_instance =
            scene_manager_parent.wait_get_field_offset(game, &il2cpp_module, "s_Instance").await as _;
        let scene_manager_current_scene_controller = scene_manager_class.wait_get_field_offset(
            game,
            &il2cpp_module,
            "<CurrentSceneController>k__BackingField",
        ).await as _;
        let scene_manager_next_scene_name = scene_manager_class.wait_get_field_offset(
            game,
            &il2cpp_module,
            "<NextSceneName>k__BackingField",
        ).await as _;

        let game_scene_controller =
            game_assembly.wait_get_class(game, &il2cpp_module, "GameSceneControllerBase").await;
        let game_scene_controller_stage_info =
            game_scene_controller.wait_get_field_offset(game, &il2cpp_module, "stageInfo").await as _;
        let game_scene_controller_is_result_sequence =
            game_scene_controller.wait_get_field_offset(game, &il2cpp_module, "isResultSequence").await as _;

        let is_loading =
            DeepPointer::new_64bit(scene_manager_static, &[scene_manager_is_in_transition]);
        let level_id = DeepPointer::new_64bit(
            scene_manager_parent_static,
            &[
                scene_manager_parent_instance,
                scene_manager_current_scene_controller,
                game_scene_controller_stage_info,
                0x0,
            ],
        );
        let is_result_sequence = DeepPointer::new_64bit(
            scene_manager_parent_static,
            &[
                scene_manager_parent_instance,
                scene_manager_current_scene_controller,
                game_scene_controller_is_result_sequence,
            ],
        );
        let next_scene_name = DeepPointer::new_64bit(
            scene_manager_parent_static,
            &[
                scene_manager_parent_instance,
                scene_manager_next_scene_name,
                0x14,
            ],
        );

        Self {
            scene_manager,
            is_loading,
            level_id,
            next_scene_name,
            is_result_sequence,
        }
    }
}

fn update_loop(game: &Process, addresses: &Memory, watchers: &mut Watchers) {
    watchers.start_trigger.update_infallible(
        if let Ok(scene_path) = addresses.scene_manager.get_current_scene_path::<128>(game) {
            let scene_name = get_scene_name(&scene_path);
            if scene_name == b"MainMenu" {
                let next_scene = addresses
                    .next_scene_name
                    .deref::<[u16; 10]>(game)
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

    watchers
        .level_id
        .update_infallible(addresses.level_id.deref(game).unwrap_or_default());

    watchers
        .is_loading
        .update_infallible(addresses.is_loading.deref(game).unwrap_or_default());

    watchers
        .goal_ring_flag
        .update_infallible(addresses.is_result_sequence.deref(game).unwrap_or_default());
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
