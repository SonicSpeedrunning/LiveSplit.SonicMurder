#![no_std]
use asr::{signature::Signature, timer, timer::TimerState, watcher::Watcher, Address, Process, time::Duration};

#[cfg(all(not(test), target_arch = "wasm32"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

static AUTOSPLITTER: spinning_top::Spinlock<State> = spinning_top::const_spinlock(State {
    game: None,
    settings: None,
    watchers: Watchers {
        dialogue: Watcher::new(),
    },
});

struct State {
    game: Option<ProcessInfo>,
    settings: Option<Settings>,
    watchers: Watchers,
}

struct ProcessInfo {
    game: Process,
    unity_module_base: Address,
    unity_module_size: u64,
    addresses: Option<MemoryPtr>,
}

struct Watchers {
    dialogue: Watcher<[u16; 90]>,
}

struct MemoryPtr {
    dialogue_base_address: Address,
}


#[derive(asr::Settings)]
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

impl ProcessInfo {
    fn attach_process() -> Option<Self> {
        const PROCESS_NAMES: [&str; 1] = ["The Murder of Sonic The Hedgehog.exe"];
        const UNITY: &str = "UnityPlayer.dll";
        let mut proc: Option<Process> = None;
    
        for name in PROCESS_NAMES {
            proc = Process::attach(name);
            if proc.is_some() {
                break
            }
        }

        let game = proc?;
        let unity_module_base = game.get_module_address(UNITY).ok()?;
        let unity_module_size = game.get_module_size(UNITY).ok()?;

        Some(Self {
            game,
            unity_module_base,
            unity_module_size,
            addresses: None,
        })
    }

    fn look_for_addresses(&mut self) -> Option<MemoryPtr> {
        const SIG: Signature<10> = Signature::new("48 8B 05 ?? ?? ?? ?? 8B 40 60");
        let game = &self.game;

        let mut ptr = SIG.scan_process_range(game, self.unity_module_base, self.unity_module_size)?.0 + 3;
        ptr += 0x4 + game.read::<u32>(Address(ptr)).ok()? as u64;

        Some(MemoryPtr {
            dialogue_base_address: Address(ptr),
        })
    }
}

impl State {
    fn init(&mut self) -> bool {        
        if self.game.is_none() {
            self.game = ProcessInfo::attach_process()
        }

        let Some(game) = &mut self.game else {
            return false
        };

        if !game.game.is_open() {
            self.game = None;
            return false
        }

        if game.addresses.is_none() {
            game.addresses = game.look_for_addresses()
        }

        game.addresses.is_some()   
    }

    fn update(&mut self) {
        let Some(game) = &self.game else { return };
        let Some(addresses) = &game.addresses else { return };
        let proc = &game.game;

        let mut dialogue: [u16; 90] = [0; 90];

        if let Ok(addr_base) = proc.read::<u64>(addresses.dialogue_base_address) {
            if let Ok(addr_1) = proc.read::<u64>(Address(addr_base + 0xB0)) {
                if let Ok(addr_2) = proc.read::<u64>(Address(addr_1 + 0xD70)) {
                    if let Ok(addr_3) = proc.read::<u64>(Address(addr_2 + 0x80)) {
                        if let Ok(st) = proc.read::<[u16; 90]>(Address(addr_3 + 0x14)) {
                            let mut i = 0;
                            for val in st {
                                if val == 0 {
                                    break
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

        self.watchers.dialogue.update(Some(dialogue));
    }

    fn start(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        if !settings.start { return false };

        let Some(dialogue) = &self.watchers.dialogue.pair else { return false };
        dialogue.changed() && dialogue.current == START_SCRIBBLE
    }

    fn split(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        let Some(dialogue) = &self.watchers.dialogue.pair else { return false };

        dialogue.changed() && match dialogue.old {
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
    }

    fn reset(&mut self) -> bool {
        let Some(settings) = &self.settings else { return false };
        if !settings.reset { return false };
        let Some(dialogue) = &self.watchers.dialogue.pair else { return false };
        dialogue.changed() && dialogue.current == RESET
    }

    fn is_loading(&mut self) -> Option<bool> {
        None
    }

    fn game_time(&mut self) -> Option<Duration> {
        None
    }
}

#[no_mangle]
pub extern "C" fn update() {
    // Get access to the spinlock
    let autosplitter = &mut AUTOSPLITTER.lock();
    
    // Sets up the settings
    autosplitter.settings.get_or_insert_with(Settings::register);

    // Main autosplitter logic, essentially refactored from the OG LivaSplit autosplitting component.
    // First of all, the autosplitter needs to check if we managed to attach to the target process,
    // otherwise there's no need to proceed further.
    if !autosplitter.init() {
        return
    }

    // The main update logic is launched with this
    autosplitter.update();

    // Splitting logic. Adapted from OG LiveSplit:
    // Order of execution
    // 1. update() [this is launched above] will always be run first. There are no conditions on the execution of this action.
    // 2. If the timer is currently either running or paused, then the isLoading, gameTime, and reset actions will be run.
    // 3. If reset does not return true, then the split action will be run.
    // 4. If the timer is currently not running (and not paused), then the start action will be run.
    if timer::state() == TimerState::Running || timer::state() == TimerState::Paused {
        if let Some(is_loading) = autosplitter.is_loading() {
            if is_loading {
                timer::pause_game_time()
            } else {
                timer::resume_game_time()
            }
        }

        if let Some(game_time) = autosplitter.game_time() {
            timer::set_game_time(game_time)
        }

        if autosplitter.reset() {
            timer::reset()
        } else if autosplitter.split() {
            timer::split()
        }
    } 

    if timer::state() == TimerState::NotRunning {
        if autosplitter.start() {
            timer::start();

            if let Some(is_loading) = autosplitter.is_loading() {
                if is_loading {
                    timer::pause_game_time()
                } else {
                    timer::resume_game_time()
                }
            }
        }
    }     
}

const START_SCRIBBLE: [u16; 90] = [0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28, 0x48, 0x6F, 0x70, 0x65, 0x20, 0x70, 0x61, 0x73, 0x73, 0x65, 0x6E, 0x67, 0x65, 0x72, 0x73, 0x20, 0x63, 0x61, 0x6E, 0x20, 0x72, 0x65, 0x61, 0x64, 0x20, 0x6D, 0x79, 0x20, 0x73, 0x63, 0x72, 0x69, 0x62, 0x62, 0x6C, 0x65, 0x2026, 0x29, 0x3C, 0x2F, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const RESET: [u16; 90] = [0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28, 0x50, 0x68, 0x65, 0x77, 0x2C, 0x20, 0x6D, 0x61, 0x64, 0x65, 0x20, 0x69, 0x74, 0x20, 0x6F, 0x6E, 0x20, 0x74, 0x68, 0x65, 0x20, 0x74, 0x72, 0x61, 0x69, 0x6E, 0x20, 0x66, 0x69, 0x66, 0x74, 0x65, 0x65, 0x6E, 0x20, 0x6D, 0x69, 0x6E, 0x75, 0x74, 0x65, 0x73, 0x20, 0x61, 0x68, 0x65, 0x61, 0x64, 0x20, 0x6F, 0x66, 0x20, 0x73, 0x63, 0x68, 0x65, 0x64, 0x75, 0x6C, 0x65, 0x2E, 0x29, 0x3C, 0x2F, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0, 0];
const STATION_END: [u16; 90] = [0x45, 0x76, 0x65, 0x72, 0x79, 0x6F, 0x6E, 0x65, 0x2C, 0x20, 0x74, 0x6F, 0x20, 0x79, 0x6F, 0x75, 0x72, 0x20, 0x73, 0x74, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_AMY: [u16; 90] = [0x3C, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3D, 0x54, 0x68, 0x6F, 0x75, 0x67, 0x68, 0x74, 0x3E, 0x28, 0x49, 0x2019, 0x6C, 0x6C, 0x20, 0x6B, 0x65, 0x65, 0x70, 0x20, 0x65, 0x76, 0x65, 0x72, 0x79, 0x6F, 0x6E, 0x65, 0x20, 0x73, 0x61, 0x66, 0x65, 0x20, 0x43, 0x6F, 0x6E, 0x64, 0x75, 0x63, 0x74, 0x6F, 0x72, 0x2C, 0x20, 0x79, 0x6F, 0x75, 0x2019, 0x6C, 0x6C, 0x20, 0x73, 0x65, 0x65, 0x2E, 0x29, 0x3C, 0x2F, 0x73, 0x74, 0x79, 0x6C, 0x65, 0x3E, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_KNUCKLES: [u16; 90] = [0x4F, 0x6E, 0x77, 0x61, 0x72, 0x64, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_ESPIO_VECTOR: [u16; 90] = [0x4F, 0x6B, 0x61, 0x79, 0x21, 0x20, 0x54, 0x68, 0x65, 0x20, 0x69, 0x6E, 0x76, 0x65, 0x73, 0x74, 0x69, 0x67, 0x61, 0x74, 0x69, 0x6F, 0x6E, 0x20, 0x63, 0x6F, 0x6E, 0x74, 0x69, 0x6E, 0x75, 0x65, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_BLAZE_ROUGE: [u16; 90] = [0x4C, 0x65, 0x74, 0x27, 0x73, 0x20, 0x64, 0x6F, 0x20, 0x69, 0x74, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_SHADOW: [u16; 90] = [0x49, 0x74, 0x27, 0x73, 0x20, 0x6E, 0x6F, 0x77, 0x20, 0x6F, 0x72, 0x20, 0x6E, 0x65, 0x76, 0x65, 0x72, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const END_SONIC: [u16; 90] = [0x41, 0x68, 0x68, 0x21, 0x20, 0x41, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x48, 0x21, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const SONIC_CHASE: [u16; 90] = [0x54, 0x69, 0x6D, 0x65, 0x20, 0x74, 0x6F, 0x20, 0x66, 0x69, 0x6E, 0x69, 0x73, 0x68, 0x20, 0x74, 0x68, 0x69, 0x73, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const ENDING: [u16; 90] = [0x59, 0x65, 0x61, 0x68, 0x2026, 0x20, 0x74, 0x68, 0x61, 0x74, 0x2019, 0x73, 0x20, 0x6A, 0x75, 0x73, 0x74, 0x20, 0x62, 0x65, 0x65, 0x6E, 0x20, 0x6D, 0x79, 0x20, 0x6C, 0x69, 0x66, 0x65, 0x21, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];