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
    game_engine::unity::il2cpp::{Module, Version},
    string::ArrayCString,
    time::Duration,
    timer::{self, TimerState},
    watcher::Watcher,
    Address, Address64, Process,
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
    is_loading: DeepPointer<1>,
    next_scene_name: DeepPointer<3>,
    current_scene_controller: DeepPointer<2>,
    game_scene_controller_offsets: GameSceneControllerOffsets,
}

struct GameSceneControllerOffsets {
    stage_info: u32,
    is_goal_sequence: u32,
    is_result_sequence: u32,
    is_time_attack_mode: u32,
}

impl Memory {
    async fn init(game: &Process) -> Self {
        let il2cpp_module = Module::wait_attach(game, Version::V2020).await;
        let game_assembly = il2cpp_module.wait_get_default_image(game).await;

        let scene_manager_class = game_assembly
            .wait_get_class(game, &il2cpp_module, "Scene_Manager")
            .await;
        let scene_manager_static = scene_manager_class
            .wait_get_static_table(game, &il2cpp_module)
            .await;
        let scene_manager_is_in_transition = scene_manager_class
            .wait_get_field_offset(game, &il2cpp_module, "<IsTransitionPlay>k__BackingField")
            .await as _;

        let is_loading =
            DeepPointer::new_64bit(scene_manager_static, &[scene_manager_is_in_transition]);

        let scene_manager_parent = scene_manager_class
            .wait_get_parent(game, &il2cpp_module)
            .await
            .wait_get_parent(game, &il2cpp_module)
            .await;
        let scene_manager_parent_static = scene_manager_parent
            .wait_get_static_table(game, &il2cpp_module)
            .await;
        let scene_manager_parent_instance = scene_manager_parent
            .wait_get_field_offset(game, &il2cpp_module, "s_Instance")
            .await as _;
        let scene_manager_current_scene_controller = scene_manager_class
            .wait_get_field_offset(
                game,
                &il2cpp_module,
                "<CurrentSceneController>k__BackingField",
            )
            .await as _;

        let current_scene_controller = DeepPointer::new_64bit(
            scene_manager_parent_static,
            &[
                scene_manager_parent_instance,
                scene_manager_current_scene_controller,
            ],
        );

        let scene_manager_next_scene_name = scene_manager_class
            .wait_get_field_offset(game, &il2cpp_module, "<NextSceneName>k__BackingField")
            .await as _;

        let next_scene_name = DeepPointer::new_64bit(
            scene_manager_parent_static,
            &[
                scene_manager_parent_instance,
                scene_manager_next_scene_name,
                0x14,
            ],
        );

        let game_scene_controller = game_assembly
            .wait_get_class(game, &il2cpp_module, "GameSceneControllerBase")
            .await;
        let game_scene_controller_stage_info = game_scene_controller
            .wait_get_field_offset(game, &il2cpp_module, "stageInfo")
            .await as _;
        let game_scene_controller_is_goal_sequence = game_scene_controller
            .wait_get_field_offset(game, &il2cpp_module, "isGoalSequence")
            .await as _;
        let game_scene_controller_is_result_sequence = game_scene_controller
            .wait_get_field_offset(game, &il2cpp_module, "isResultSequence")
            .await as _;

        let game_scene_controller = game_assembly
            .wait_get_class(game, &il2cpp_module, "GameSceneController")
            .await;
        let game_scene_controller_is_time_attack_mode = game_scene_controller
            .wait_get_field_offset(game, &il2cpp_module, "isTimeAttackMode")
            .await as _;

        let game_scene_controller_offsets = GameSceneControllerOffsets {
            stage_info: game_scene_controller_stage_info,
            is_goal_sequence: game_scene_controller_is_goal_sequence,
            is_result_sequence: game_scene_controller_is_result_sequence,
            is_time_attack_mode: game_scene_controller_is_time_attack_mode,
        };

        Self {
            is_loading,
            next_scene_name,
            current_scene_controller,
            game_scene_controller_offsets,
        }
    }

    fn get_current_scene_controller_name<const N: usize>(
        &self,
        game: &Process,
        scene_controller_address: Address,
    ) -> Option<ArrayCString<N>> {
        game.read_pointer_path64(scene_controller_address, &[0, 0x10, 0])
            .ok()
    }
}

fn update_loop(game: &Process, addresses: &Memory, watchers: &mut Watchers) {
    let current_scene_controller: Address = addresses
        .current_scene_controller
        .deref::<Address64>(game)
        .unwrap_or_default()
        .into();
    let current_scene_controller_name = addresses
        .get_current_scene_controller_name::<128>(game, current_scene_controller)
        .unwrap_or_default();

    watchers.start_trigger.update_infallible(
        current_scene_controller_name.matches(b"SelectSaveSlotController")
            && addresses
                .next_scene_name
                .deref::<[u16; 10]>(game)
                .is_ok_and(|name| &name.map(|val| val as u8) == b"MovieScene"),
    );

    watchers.level_id.update_infallible(
        if current_scene_controller_name.matches(b"GameSceneController") {
            game.read_pointer_path64(
                current_scene_controller,
                &[addresses.game_scene_controller_offsets.stage_info as _, 0],
            )
            .unwrap_or_default()
        } else {
            match &watchers.level_id.pair {
                Some(x) => x.current,
                _ => 0,
            }
        },
    );

    watchers
        .is_loading
        .update_infallible(addresses.is_loading.deref(game).unwrap_or_default());

    watchers.goal_ring_flag.update_infallible(
        if current_scene_controller_name.matches(b"GameSceneController") {
            let is_time_attack = game.read::<bool>(
                current_scene_controller
                    + addresses.game_scene_controller_offsets.is_time_attack_mode,
            );
            if is_time_attack.is_ok_and(|val| val) {
                false
            } else {
                game.read(
                    current_scene_controller
                        + addresses.game_scene_controller_offsets.is_result_sequence,
                )
                .is_ok_and(|val| val)
            }
        } else {
            match &watchers.goal_ring_flag.pair {
                Some(x) => x.current,
                _ => false,
            }
        },
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
