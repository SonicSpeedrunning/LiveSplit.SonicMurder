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
    file_format::pe,
    future::{next_tick, retry},
    settings::Gui,
    signature::Signature,
    time::Duration,
    timer::{self, TimerState},
    watcher::Watcher,
    Address, Address64, Process,
};

asr::panic_handler!();
asr::async_main!(nightly);

const PROCESS_NAMES: &[&str] = &["The Murder of Sonic The Hedgehog.exe"];

async fn main() {
    let mut settings = Settings::register();

    loop {
        // Hook to the target process
        let process = retry(|| PROCESS_NAMES.iter().find_map(|&name| Process::attach(name))).await;

        process
            .until_closes(async {
                // Once the target has been found and attached to, set up some default watchers
                let mut watchers = Watchers::default();

                // Perform memory scanning to look for the addresses we need
                let addresses = Addresses::init(&process).await;

                loop {
                    // Splitting logic. Adapted from OG LiveSplit:
                    // Order of execution
                    // 1. update() will always be run first. There are no conditions on the execution of this action.
                    // 2. If the timer is currently either running or paused, then the isLoading, gameTime, and reset actions will be run.
                    // 3. If reset does not return true, then the split action will be run.
                    // 4. If the timer is currently not running (and not paused), then the start action will be run.
                    settings.update();
                    update_loop(&process, &addresses, &mut watchers);

                    let timer_state = timer::state();
                    if timer_state == TimerState::Running || timer_state == TimerState::Paused {
                        if let Some(is_loading) = is_loading(&watchers, &settings) {
                            if is_loading {
                                timer::pause_game_time()
                            } else {
                                timer::resume_game_time()
                            }
                        }

                        if let Some(game_time) = game_time(&watchers, &settings) {
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
    /// START: Enable auto start
    start: bool,
    #[default = true]
    /// RESET: Enable auto reset
    reset: bool,
    #[default = true]
    /// Dining Car (Prologue)
    station: bool,
    #[default = true]
    /// Dining Closet (Amy)
    closet: bool,
    #[default = true]
    /// Saloon Car (Knuckles)
    saloon: bool,
    #[default = true]
    /// Library Car (Espio & Vector)
    library: bool,
    #[default = true]
    /// Casino Car (Blaze & Rouge)
    casino: bool,
    #[default = true]
    /// Lounge Car (Shadow)
    lounge: bool,
    #[default = true]
    /// Conductor Car (Sonic)
    conductor: bool,
    #[default = true]
    /// Sonic Chase
    sonic_chase: bool,
    #[default = true]
    /// Train boss fight and ending
    ending: bool,
}

#[derive(Default)]
struct Watchers {
    dialogue: Watcher<[u16; 90]>,
}

struct Addresses {
    dialogue_base_address: Address,
}

impl Addresses {
    async fn init(process: &Process) -> Self {
        let unity_module = {
            let main_module_base = retry(|| process.get_module_address("UnityPlayer.dll")).await;
            let main_module_size =
                retry(|| pe::read_size_of_image(process, main_module_base)).await as u64;
            (main_module_base, main_module_size)
        };

        let ptr = {
            const SIG: Signature<10> = Signature::new("48 8B 05 ?? ?? ?? ?? 8B 40 60");
            let ptr = retry(|| SIG.scan_process_range(process, unity_module)).await + 3;
            ptr + 0x4 + retry(|| process.read::<i32>(ptr)).await
        };

        Self {
            dialogue_base_address: ptr,
        }
    }
}

fn update_loop(proc: &Process, addresses: &Addresses, watchers: &mut Watchers) {
    let mut dialogue: [u16; 90] = [0; 90];

    if let Ok(addr_base) = proc.read::<Address64>(addresses.dialogue_base_address) {
        if let Ok(addr_1) = proc.read::<Address64>(addr_base + 0xB0) {
            if let Ok(addr_2) = proc.read::<Address64>(addr_1 + 0xD70) {
                if let Ok(addr_3) = proc.read::<Address64>(addr_2 + 0x80) {
                    if let Ok(st) = proc.read::<[u16; 90]>(addr_3 + 0x14) {
                        let mut i = 0;
                        for val in st {
                            if val == 0 {
                                break;
                            } else {
                                dialogue[i] = val;
                                i += 1
                            }
                        }
                    }
                }
            }
        }
    }

    watchers.dialogue.update(Some(dialogue));
}

fn start(watchers: &Watchers, settings: &Settings) -> bool {
    settings.start
        && watchers
            .dialogue
            .pair
            .is_some_and(|val| val.changed_to(&START_SCRIBBLE))
}

fn split(watchers: &Watchers, settings: &Settings) -> bool {
    watchers.dialogue.pair.is_some_and(|val| {
        val.changed()
            && match val.old {
                STATION_END => settings.station,
                END_AMY => settings.closet,
                END_KNUCKLES => settings.saloon,
                END_ESPIO_VECTOR => settings.library,
                END_BLAZE_ROUGE => settings.casino,
                END_SHADOW => settings.lounge,
                END_SONIC => settings.conductor,
                SONIC_CHASE => settings.sonic_chase,
                ENDING => settings.ending,
                _ => false,
            }
    })
}

fn reset(watchers: &Watchers, settings: &Settings) -> bool {
    settings.reset
        && watchers
            .dialogue
            .pair
            .is_some_and(|val| val.changed_to(&RESET))
}

fn is_loading(_watchers: &Watchers, _settings: &Settings) -> Option<bool> {
    None
}

fn game_time(_watchers: &Watchers, _settings: &Settings) -> Option<Duration> {
    None
}

const START_SCRIBBLE: [u16; 90] = [
    0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28,
    0x48, 0x6F, 0x70, 0x65, 0x20, 0x70, 0x61, 0x73, 0x73, 0x65, 0x6E, 0x67, 0x65, 0x72, 0x73, 0x20,
    0x63, 0x61, 0x6E, 0x20, 0x72, 0x65, 0x61, 0x64, 0x20, 0x6D, 0x79, 0x20, 0x73, 0x63, 0x72, 0x69,
    0x62, 0x62, 0x6C, 0x65, 0x2026, 0x29, 0x3C, 0x2F, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const RESET: [u16; 90] = [
    0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28,
    0x50, 0x68, 0x65, 0x77, 0x2C, 0x20, 0x6D, 0x61, 0x64, 0x65, 0x20, 0x69, 0x74, 0x20, 0x6F, 0x6E,
    0x20, 0x74, 0x68, 0x65, 0x20, 0x74, 0x72, 0x61, 0x69, 0x6E, 0x20, 0x66, 0x69, 0x66, 0x74, 0x65,
    0x65, 0x6E, 0x20, 0x6D, 0x69, 0x6E, 0x75, 0x74, 0x65, 0x73, 0x20, 0x61, 0x68, 0x65, 0x61, 0x64,
    0x20, 0x6F, 0x66, 0x20, 0x73, 0x63, 0x68, 0x65, 0x64, 0x75, 0x6C, 0x65, 0x2E, 0x29, 0x3C, 0x2F,
    0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0, 0,
];
const STATION_END: [u16; 90] = [
    0x45, 0x76, 0x65, 0x72, 0x79, 0x6F, 0x6E, 0x65, 0x2C, 0x20, 0x74, 0x6F, 0x20, 0x79, 0x6F, 0x75,
    0x72, 0x20, 0x73, 0x74, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const END_AMY: [u16; 90] = [
    0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28,
    0x49, 0x2019, 0x6C, 0x6C, 0x20, 0x6B, 0x65, 0x65, 0x70, 0x20, 0x65, 0x76, 0x65, 0x72, 0x79,
    0x6F, 0x6E, 0x65, 0x20, 0x73, 0x61, 0x66, 0x65, 0x20, 0x43, 0x6F, 0x6E, 0x64, 0x75, 0x63, 0x74,
    0x6F, 0x72, 0x2C, 0x20, 0x79, 0x6F, 0x75, 0x2019, 0x6C, 0x6C, 0x20, 0x73, 0x65, 0x65, 0x2E,
    0x29, 0x3C, 0x2F, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0,
];
const END_KNUCKLES: [u16; 90] = [
    0x4F, 0x6E, 0x77, 0x61, 0x72, 0x64, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0,
];
const END_ESPIO_VECTOR: [u16; 90] = [
    0x4F, 0x6B, 0x61, 0x79, 0x21, 0x20, 0x54, 0x68, 0x65, 0x20, 0x69, 0x6E, 0x76, 0x65, 0x73, 0x74,
    0x69, 0x67, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x20, 0x63, 0x6F, 0x6E, 0x74, 0x69, 0x6E, 0x75, 0x65,
    0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const END_BLAZE_ROUGE: [u16; 90] = [
    0x4C, 0x65, 0x74, 0x27, 0x73, 0x20, 0x64, 0x6F, 0x20, 0x69, 0x74, 0x21, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0,
];
const END_SHADOW: [u16; 90] = [
    0x49, 0x74, 0x27, 0x73, 0x20, 0x6E, 0x6F, 0x77, 0x20, 0x6F, 0x72, 0x20, 0x6E, 0x65, 0x76, 0x65,
    0x72, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const END_SONIC: [u16; 90] = [
    0x41, 0x68, 0x68, 0x21, 0x20, 0x41, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x21,
    0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const SONIC_CHASE: [u16; 90] = [
    0x54, 0x69, 0x6D, 0x65, 0x20, 0x74, 0x6F, 0x20, 0x66, 0x69, 0x6E, 0x69, 0x73, 0x68, 0x20, 0x74,
    0x68, 0x69, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
const ENDING: [u16; 90] = [
    0x59, 0x65, 0x61, 0x68, 0x2026, 0x20, 0x74, 0x68, 0x61, 0x74, 0x2019, 0x73, 0x20, 0x6A, 0x75,
    0x73, 0x74, 0x20, 0x62, 0x65, 0x65, 0x6E, 0x20, 0x6D, 0x79, 0x20, 0x6C, 0x69, 0x66, 0x65, 0x21,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];
