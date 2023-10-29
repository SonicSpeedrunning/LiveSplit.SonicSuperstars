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
    settings::Gui,
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
    let mut settings = Settings::register();

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
                    settings.update();
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

#[derive(Gui)]
struct Settings {
    #[default = true]
    /// => AUTO START: Enable auto start (Story Mode)
    start_story: bool,
    #[default = true]
    /// => AUTO START: Enable auto start (Trip's Story)
    start_trip: bool,
    #[default = true]
    /// => AUTO START: Enable auto start (Last Story)
    start_last_story: bool,
    #[default = false]
    /// ---------- STORY MODE ----------
    _story: bool,
    #[default = true]
    /// Bridge Island Zone - Act 1
    bridge_island_1: bool,
    #[default = true]
    /// Bridge Island Zone - Act 2
    bridge_island_2: bool,
    #[default = true]
    /// Bridge Island Zone - Act Fruit
    bridge_island_fruit: bool,
    #[default = true]
    /// Speed Jungle Zone - Act 1
    speed_jungle_1: bool,
    #[default = true]
    /// Speed Jungle Zone - Act Sonic
    speed_jungle_sonic: bool,
    #[default = true]
    /// Speed Jungle Zone - Act 2
    speed_jungle_2: bool,
    #[default = true]
    /// Sky Temple Zone - Act 1
    sky_temple_1: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act 1
    pinball_carnival_1: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act 2
    pinball_carnival_2: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act Fruit
    pinball_carnival_fruit: bool,
    #[default = true]
    /// Lagoon City Zone - Act 1
    lagoon_city_1: bool,
    #[default = true]
    /// Lagoon City Zone - Act Amy
    lagoon_city_amy: bool,
    #[default = true]
    /// Lagoon City Zone - Act 2
    lagoon_city_2: bool,
    #[default = true]
    /// Sand Sanctuary Zone - Act 1
    sand_sanctuary_1: bool,
    #[default = true]
    /// Press Factory Zone - Act 1
    press_factory_1: bool,
    #[default = true]
    /// Press Factory Zone - Act 2
    press_factory_2: bool,
    #[default = true]
    /// Press Factory Zone - Act Fruit
    press_factory_fruit: bool,
    #[default = true]
    /// Golden Capital Zone - Act 1
    golden_capital_1: bool,
    #[default = true]
    /// Golden Capital Zone - Act Knuckles
    golden_capital_knuckles: bool,
    #[default = true]
    /// Golden Capital Zone - Act 2
    golden_capital_2: bool,
    #[default = true]
    /// Cyber Station Zone - Act 1
    cyber_station_1: bool,
    #[default = true]
    /// Frozen Base Zone - Act 1
    frozen_base_1: bool,
    #[default = true]
    /// Frozen Base Zone - Act Tails
    frozen_base_tails: bool,
    #[default = true]
    /// Frozen Base Zone - Act 2
    frozen_base_2: bool,
    #[default = true]
    /// Egg Fortress Zone - Act 1
    egg_fortress_1: bool,
    #[default = true]
    /// Egg Fortress Zone - Act 2
    egg_fortress_2: bool,
    #[default = false]
    /// ---------- TRIP'S STORY ----------
    _trip: bool,
    #[default = true]
    /// Bridge Island Zone - Act 1
    trip_bridge_island_1: bool,
    #[default = true]
    /// Bridge Island Zone - Act 2
    trip_bridge_island_2: bool,
    #[default = true]
    /// Bridge Island Zone - Act Fruit
    trip_bridge_island_fruit: bool,
    #[default = true]
    /// Speed Jungle Zone - Act 1
    trip_speed_jungle_1: bool,
    #[default = true]
    /// Speed Jungle Zone - Act 2
    trip_speed_jungle_2: bool,
    #[default = true]
    /// Speed Jungle Zone - Act 3
    trip_speed_jungle_3: bool,
    #[default = true]
    /// Sky Temple Zone - Act 1
    trip_sky_temple_1: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act 1
    trip_pinball_carnival_1: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act 2
    trip_pinball_carnival_2: bool,
    #[default = true]
    /// Pinball Carnival Zone - Act Fruit
    trip_pinball_carnival_fruit: bool,
    #[default = true]
    /// Lagoon City Zone - Act 1
    trip_lagoon_city_1: bool,
    #[default = true]
    /// Lagoon City Zone - Act 2
    trip_lagoon_city_2: bool,
    #[default = true]
    /// Lagoon City Zone - Act 3
    trip_lagoon_city_3: bool,
    #[default = true]
    /// Sand Sanctuary Zone - Act 1
    trip_sand_sanctuary_1: bool,
    #[default = true]
    /// Press Factory Zone - Act 1
    trip_press_factory_1: bool,
    #[default = true]
    /// Press Factory Zone - Act 2
    trip_press_factory_2: bool,
    #[default = true]
    /// Press Factory Zone - Act Fruit
    trip_press_factory_fruit: bool,
    #[default = true]
    /// Golden Capital Zone - Act 1
    trip_golden_capital_1: bool,
    #[default = true]
    /// Golden Capital Zone - Act 2
    trip_golden_capital_2: bool,
    #[default = true]
    /// Golden Capital Zone - Act 3
    trip_golden_capital_3: bool,
    #[default = true]
    /// Cyber Station Zone - Act 1
    trip_cyber_station_1: bool,
    #[default = true]
    /// Frozen Base Zone - Act 1
    trip_frozen_base_1: bool,
    #[default = true]
    /// Frozen Base Zone - Act 2
    trip_frozen_base_2: bool,
    #[default = true]
    /// Frozen Base Zone - Act 3
    trip_frozen_base_3: bool,
    #[default = true]
    /// Egg Fortress Zone - Act 1
    trip_egg_fortress_1: bool,
    #[default = true]
    /// Egg Fortress Zone - Act 2
    trip_egg_fortress_2: bool,
    #[default = false]
    /// ---------- FINAL STORY ----------
    _final_story: bool,
    #[default = true]
    /// Defeat the black dragon
    black_dragon: bool,
}

#[derive(Default)]
struct Watchers {
    start_trigger: Watcher<bool>,
    start_trigger_trip: Watcher<bool>,
    game_mode: Watcher<u32>,
    level_id: Watcher<u32>,
    is_loading: Watcher<bool>,
    goal_ring_flag: Watcher<bool>,
    boss_defeated: Watcher<bool>,
}

struct Memory {
    is_loading: DeepPointer<1>,
    game_mode: DeepPointer<2>,
    save_data: SysSaveDataStory,
    current_scene_controller: DeepPointer<2>,
    game_scene_controller_offsets: GameSceneControllerOffsets,
    boss_controller_offsets: EnemySpecialBase,
}

struct SysSaveDataStory {
    static_table: Address,
    instance: u64,
    sys_save_data: u64,
    save_datas: u64,
    current_slot: u64,
    is_normal_first_play: u64,
    is_trip_first_play: u64,
}

struct GameSceneControllerOffsets {
    stage_info: u64,
    is_goal_sequence: u64,
    is_result_sequence: u64,
    is_time_attack_mode: u64,
    active_boss_base: u64,
}

struct EnemySpecialBase {
    base_type: u64, // Becomes 3 when boss dies
}

impl Memory {
    async fn init(game: &Process) -> Self {
        let il2cpp_module = Module::wait_attach(game, Version::V2020).await;
        let game_assembly = il2cpp_module.wait_get_default_image(game).await;

        // The main class used for monitoring level progression
        let scene_manager_class = game_assembly
            .wait_get_class(game, &il2cpp_module, "Scene_Manager")
            .await;

        // We need to recover the current game mode in order to differentiate between story mode, trip story and final story
        let game_mode = {
            let sys_game_manager = game_assembly
                .wait_get_class(game, &il2cpp_module, "SysGameManager")
                .await;
            let sys_game_manager_parent = sys_game_manager
                .wait_get_parent(game, &il2cpp_module)
                .await
                .wait_get_parent(game, &il2cpp_module)
                .await;
            let game_mode = sys_game_manager
                .wait_get_field_offset(game, &il2cpp_module, "gameMode")
                .await as _;
            let static_table = sys_game_manager_parent
                .wait_get_static_table(game, &il2cpp_module)
                .await;
            let instance = sys_game_manager_parent
                .wait_get_field_offset(game, &il2cpp_module, "s_Instance")
                .await as _;
            DeepPointer::new_64bit(static_table, &[instance, game_mode])
        };

        // Self-explanatory. In reality this checks a static field inside the scene_manager class that tells us whenever we are in a transision.
        // It's a good loading variable.
        let is_loading = {
            let scene_manager_static = scene_manager_class
                .wait_get_static_table(game, &il2cpp_module)
                .await;
            let scene_manager_is_in_transition = scene_manager_class
                .wait_get_field_offset(game, &il2cpp_module, "<IsTransitionPlay>k__BackingField")
                .await as _;
            DeepPointer::new_64bit(scene_manager_static, &[scene_manager_is_in_transition])
        };

        // This is a bit of spaghetti code we use to recover the address of the current SceneController.
        // Not that this links to an abstract class, so we need to check which class that inherits from it we are currently in.
        let current_scene_controller = {
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

            DeepPointer::new_64bit(
                scene_manager_parent_static,
                &[
                    scene_manager_parent_instance,
                    scene_manager_current_scene_controller,
                ],
            )
        };

        // Save data. This class contains stuff about story progression and unlocks.
        // It also tells us if we are starting story mode / trip story the first time, making it a
        // perfect variable for triggering the start of a run.
        let save_data = {
            let sys_save_manager = game_assembly
                .wait_get_class(game, &il2cpp_module, "SysSaveManager")
                .await;
            let sys_save_manager_parent = sys_save_manager
                .wait_get_parent(game, &il2cpp_module)
                .await
                .wait_get_parent(game, &il2cpp_module)
                .await;
            let sys_save_manager_instance = sys_save_manager_parent
                .wait_get_static_table(game, &il2cpp_module)
                .await;
            let instance = sys_save_manager_parent
                .wait_get_field_offset(game, &il2cpp_module, "s_Instance")
                .await as _;
            let save_data = sys_save_manager
                .wait_get_field_offset(game, &il2cpp_module, "sysSaveData")
                .await as _;
            let current_slot = sys_save_manager
                .wait_get_field_offset(game, &il2cpp_module, "<CurrentSlotNo>k__BackingField")
                .await as _;
            let save_datas = game_assembly
                .wait_get_class(game, &il2cpp_module, "SysSaveData")
                .await
                .wait_get_field_offset(game, &il2cpp_module, "SaveDatas")
                .await as _;

            let sys_save_data_story = game_assembly
                .wait_get_class(game, &il2cpp_module, "SysSaveDataStory")
                .await;
            let is_normal_first_play = sys_save_data_story
                .wait_get_field_offset(game, &il2cpp_module, "IsNormalFirstPlay")
                .await as _;
            let is_trip_first_play = sys_save_data_story
                .wait_get_field_offset(game, &il2cpp_module, "IsTripFirstPlay")
                .await as _;

            SysSaveDataStory {
                static_table: sys_save_manager_instance,
                instance,
                sys_save_data: save_data,
                save_datas,
                current_slot,
                is_normal_first_play,
                is_trip_first_play,
            }
        };

        // The the SceneController is just an abstract class, we want to delve deeper into this
        // "GameSceneControllerBase" class in order to recover the offsets we need.
        let game_scene_controller_offsets = {
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

            let active_boss_base = game_scene_controller
                .wait_get_field_offset(game, &il2cpp_module, "activeBossBase")
                .await as _;

            GameSceneControllerOffsets {
                stage_info: game_scene_controller_stage_info,
                is_goal_sequence: game_scene_controller_is_goal_sequence,
                is_result_sequence: game_scene_controller_is_result_sequence,
                is_time_attack_mode: game_scene_controller_is_time_attack_mode,
                active_boss_base,
            }
        };

        // This reports whenever a boss dies. Currently defined without looking for its class as it's not loaded in time for the start of a run
        let boss_final = {
            let base_type = 0x130;

            /*
            let class = game_assembly
                .wait_get_class(game, &il2cpp_module, "EnemySpecialBase")
                .await;


            let base_type = class
                .wait_get_field_offset(game, &il2cpp_module, "baseType")
                .await as _;
            */

            EnemySpecialBase { base_type }
        };

        asr::print_limited::<24>(&"  => Autosplitter ready!");

        Self {
            is_loading,
            game_mode,
            save_data,
            current_scene_controller,
            game_scene_controller_offsets,
            boss_controller_offsets: boss_final,
        }
    }
}

fn update_loop(game: &Process, addresses: &Memory, watchers: &mut Watchers) {
    const GAME_SCENE_CONTROLLER_TYPES: &[&str] = &[
        "GameSceneController",
        "BlackDragonBattleGameSceneController",
        "BRMainGameSceneController",
        "BROverallResultSceneController",
        "EndingGameSceneController",
        "MiniActGameSceneController",
        "ShootingGameSceneController",
        "WorldMapGameSceneController",
    ];

    const BOSSES_TYPES: &[&str] = &["Bos111", "Bos112"];

    let current_scene_controller: Address = addresses
        .current_scene_controller
        .deref::<Address64>(game)
        .unwrap_or_default()
        .into();

    let current_scene_controller_name = game
        .read_pointer_path64::<ArrayCString<128>>(current_scene_controller, &[0, 0x10, 0])
        .unwrap_or_default();

    // The main GameSceneController (and its inherited class) are the classes we're interested in for autosplitting purposes.
    let is_game_scene = GAME_SCENE_CONTROLLER_TYPES
        .iter()
        .any(|val| current_scene_controller_name.matches(val));

    // Save data stuff we read from memory to determine if we're starting a new game
    let sys_save =
        game.read::<Address64>(addresses.save_data.static_table + addresses.save_data.instance);

    let current_slot = if let Ok(sys_save) = sys_save {
        game.read::<u32>(sys_save + addresses.save_data.current_slot)
            .unwrap_or_default()
    } else {
        0
    };

    let save_slot = {
        if let Ok(sys_save) = sys_save {
            game.read_pointer_path64::<Address64>(
                sys_save,
                &[
                    addresses.save_data.sys_save_data,
                    addresses.save_data.save_datas,
                    0x20 + current_slot.wrapping_mul(8) as u64,
                ],
            )
            .ok()
        } else {
            None
        }
    };

    watchers.start_trigger.update_infallible(!{
        if let Some(save_slot) = save_slot {
            game.read::<bool>(save_slot + addresses.save_data.is_normal_first_play)
                .unwrap_or_default()
        } else {
            false
        }
    });

    watchers.start_trigger_trip.update_infallible(!{
        if let Some(save_slot) = save_slot {
            game.read::<bool>(save_slot + addresses.save_data.is_trip_first_play)
                .unwrap_or_default()
        } else {
            false
        }
    });

    watchers.level_id.update_infallible(if is_game_scene {
        game.read_pointer_path64(
            current_scene_controller,
            &[addresses.game_scene_controller_offsets.stage_info, 0x10],
        )
        .unwrap_or_default()
    } else {
        match &watchers.level_id.pair {
            Some(x) => x.current,
            _ => 0,
        }
    });

    watchers
        .is_loading
        .update_infallible(addresses.is_loading.deref(game).unwrap_or_default());

    watchers.goal_ring_flag.update_infallible(if is_game_scene {
        let is_time_attack = game.read::<bool>(
            current_scene_controller + addresses.game_scene_controller_offsets.is_time_attack_mode,
        );

        if is_time_attack.is_ok_and(|val| val) {
            false
        } else {
            game.read(
                current_scene_controller
                    + addresses.game_scene_controller_offsets.is_result_sequence,
            )
            .is_ok_and(|val| val)
                || game
                    .read(
                        current_scene_controller
                            + addresses.game_scene_controller_offsets.is_goal_sequence,
                    )
                    .is_ok_and(|val| val)
        }
    } else {
        match &watchers.goal_ring_flag.pair {
            Some(x) => x.current,
            _ => false,
        }
    });

    watchers
        .game_mode
        .update_infallible(addresses.game_mode.deref(game).unwrap_or_default());

    watchers.boss_defeated.update_infallible({
        if game
            .read_pointer_path64::<ArrayCString<128>>(
                current_scene_controller,
                &[
                    addresses.game_scene_controller_offsets.active_boss_base,
                    0,
                    0x10,
                    0,
                ],
            )
            .is_ok_and(|val| BOSSES_TYPES.iter().any(|v| val.matches(v)))
        {
            game.read_pointer_path64::<u8>(
                current_scene_controller,
                &[
                    addresses.game_scene_controller_offsets.active_boss_base,
                    addresses.boss_controller_offsets.base_type,
                ],
            )
            .is_ok_and(|val| val == 3)
        } else {
            match &watchers.boss_defeated.pair {
                Some(x) => x.current,
                _ => false,
            }
        }
    });
}

fn start(watchers: &Watchers, settings: &Settings) -> bool {
    (settings.start_story
        && watchers
            .start_trigger
            .pair
            .is_some_and(|val| val.changed_to(&true)))
        || (settings.start_trip
            && watchers
                .start_trigger_trip
                .pair
                .is_some_and(|val| val.changed_to(&true)))
        || (settings.start_last_story
            && watchers
                .game_mode
                .pair
                .is_some_and(|val| val.changed_to(&2)))
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    let Some(game_mode) = &watchers.game_mode.pair else {
        return false;
    };
    let Some(level_id) = &watchers.level_id.pair else {
        return false;
    };
    let Some(goal_ring) = &watchers.goal_ring_flag.pair else {
        return false;
    };

    // Final boss
    if level_id.old == 110200
        && (watchers
            .boss_defeated
            .pair
            .is_some_and(|val| val.changed_to(&true))
            || goal_ring.changed_to(&true))
    {
        match game_mode.current {
            0 => return settings.egg_fortress_2,
            1 => return settings.trip_egg_fortress_2,
            _ => (),
        };
    }

    match game_mode.current {
        0 => {
            goal_ring.changed_to(&false)
                && match level_id.old {
                    10100 => settings.bridge_island_1,
                    10200 => settings.bridge_island_2,
                    600102 => settings.bridge_island_fruit,
                    20100 => settings.speed_jungle_1,
                    20200 => settings.speed_jungle_sonic,
                    20300 => settings.speed_jungle_2,
                    30100 => settings.sky_temple_1,
                    40100 => settings.pinball_carnival_1,
                    40200 => settings.pinball_carnival_2,
                    600401 => settings.pinball_carnival_fruit,
                    50100 => settings.lagoon_city_1,
                    50200 => settings.lagoon_city_amy,
                    50300 => settings.lagoon_city_2,
                    60100 => settings.sand_sanctuary_1,
                    70100 => settings.press_factory_1,
                    70200 => settings.press_factory_2,
                    600702 => settings.press_factory_fruit,
                    80100 => settings.golden_capital_1,
                    80200 => settings.golden_capital_knuckles,
                    80300 => settings.golden_capital_2,
                    90100 => settings.cyber_station_1,
                    100100 => settings.frozen_base_1,
                    100200 => settings.frozen_base_tails,
                    100300 => settings.frozen_base_2,
                    110100 => settings.egg_fortress_1,
                    110200 => settings.egg_fortress_2,
                    _ => false,
                }
        }
        1 => {
            goal_ring.changed_to(&false)
                && match level_id.old {
                    10100 => settings.trip_bridge_island_1,
                    10200 => settings.trip_bridge_island_2,
                    600102 => settings.trip_bridge_island_fruit,
                    20100 => settings.trip_speed_jungle_1,
                    20200 => settings.trip_speed_jungle_2,
                    20300 => settings.trip_speed_jungle_3,
                    30100 => settings.trip_sky_temple_1,
                    40100 => settings.trip_pinball_carnival_1,
                    40200 => settings.trip_pinball_carnival_2,
                    600401 => settings.trip_pinball_carnival_fruit,
                    50100 => settings.trip_lagoon_city_1,
                    50200 => settings.trip_lagoon_city_2,
                    50300 => settings.trip_lagoon_city_3,
                    60100 => settings.trip_sand_sanctuary_1,
                    70100 => settings.trip_press_factory_1,
                    70200 => settings.trip_press_factory_2,
                    600702 => settings.trip_press_factory_fruit,
                    80100 => settings.trip_golden_capital_1,
                    80200 => settings.trip_golden_capital_2,
                    80300 => settings.trip_golden_capital_3,
                    90100 => settings.trip_cyber_station_1,
                    100100 => settings.trip_frozen_base_1,
                    100200 => settings.trip_frozen_base_2,
                    100300 => settings.trip_frozen_base_3,
                    110100 => settings.trip_egg_fortress_1,
                    110200 => settings.trip_egg_fortress_2,
                    _ => false,
                }
        }
        2 => {
            watchers
                .boss_defeated
                .pair
                .is_some_and(|val| val.changed_to(&true))
                && settings.black_dragon
        }
        _ => false,
    }
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
