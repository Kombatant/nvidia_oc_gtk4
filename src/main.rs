use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use nvml_wrapper::{error::NvmlError, Device, Nvml};
use nvml_wrapper_sys::bindings::{
    nvmlDevice_t, nvmlReturn_enum_NVML_SUCCESS,
    nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR,
    nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX,
    nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN, NvmlLib,
};
use serde::Deserialize;
use std::{collections::HashMap, io};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    /// Path to the config file
    #[arg(short, long, default_value = "/etc/nvidia_oc.json")]
    file: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Sets GPU parameters like frequency offset and power limit
    Set {
        /// GPU index
        #[arg(short, long)]
        index: u32,

        #[command(flatten)]
        sets: Sets,
    },
    /// Gets GPU parameters
    Get {
        /// GPU index
        #[arg(short, long)]
        index: u32,
    },
    /// Generate shell completion script
    Completion {
        /// The shell to generate the script for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Args, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[group(required = true, multiple = true)]
struct Sets {
    /// GPU frequency offset
    #[arg(short, long, allow_hyphen_values = true)]
    freq_offset: Option<i32>,
    /// GPU memory frequency offset
    #[arg(long, allow_hyphen_values = true)]
    mem_offset: Option<i32>,
    /// GPU power limit in milliwatts
    #[arg(short, long)]
    power_limit: Option<u32>,
    /// GPU min clock
    #[arg(long, requires = "max_clock")]
    min_clock: Option<u32>,
    /// GPU max clock
    #[arg(long, requires = "min_clock")]
    max_clock: Option<u32>,
    /// GPU min memory clock
    #[arg(long, requires = "max_mem_clock")]
    min_mem_clock: Option<u32>,
    /// GPU max memory clock
    #[arg(long, requires = "min_mem_clock")]
    max_mem_clock: Option<u32>,
    /// Target temperature in Celsius (acoustic limit). The GPU will automatically
    /// adjust fan curves to maintain this temperature. Similar to MSI Afterburner's
    /// "target temperature" feature.
    #[arg(short, long)]
    target_temp: Option<u32>,
}

impl Sets {
    fn apply(&self, device: &mut Device) {
        if let Some(freq_offset) = self.freq_offset {
            device
                .set_gpc_clock_vf_offset(freq_offset)
                .expect("Failed to set GPU frequency offset");
        }

        if let Some(mem_offset) = self.mem_offset {
            device
                .set_mem_clock_vf_offset(mem_offset)
                .expect("Failed to set GPU memory frequency offset");
        }

        if let Some(limit) = self.power_limit {
            if let Err(e) = device.set_power_management_limit(limit) {
                match e {
                    NvmlError::InvalidArg => {
                        let mut error_msg = format!(
                            "Failed to set GPU power limit: {} mW is out of range.",
                            limit
                        );
                        if let Ok(constraints) = device.power_management_limit_constraints() {
                            error_msg.push_str(&format!(
                                " Valid range: {}-{} mW ({}-{} W)",
                                constraints.min_limit,
                                constraints.max_limit,
                                constraints.min_limit / 1000,
                                constraints.max_limit / 1000
                            ));
                        }
                        panic!("{}", error_msg);
                    }
                    _ => panic!("Failed to set GPU power limit: {:?}", e),
                }
            }
        }

        if let (Some(min_clock), Some(max_clock)) = (self.min_clock, self.max_clock) {
            device
                .set_gpu_locked_clocks(
                    nvml_wrapper::enums::device::GpuLockedClocksSetting::Numeric {
                        min_clock_mhz: min_clock,
                        max_clock_mhz: max_clock,
                    },
                )
                .expect("Failed to set GPU min and max clocks");
        }

        if let (Some(min_mem_clock), Some(max_mem_clock)) = (self.min_mem_clock, self.max_mem_clock)
        {
            device
                .set_mem_locked_clocks(min_mem_clock, max_mem_clock)
                .expect("Failed to set GPU min and max memory clocks");
        }

        if let Some(target_temp) = self.target_temp {
            if let Err(e) = set_acoustic_temperature(device, target_temp) {
                let (min, max) = get_acoustic_temperature_range(device);
                let mut error_msg = format!(
                    "Failed to set target temperature: {}°C - {}",
                    target_temp, e
                );
                if let (Some(min), Some(max)) = (min, max) {
                    error_msg.push_str(&format!(" Valid range: {}°C - {}°C", min, max));
                }
                panic!("{}", error_msg);
            }
        }
    }
}

#[derive(Deserialize)]
struct Config {
    sets: HashMap<u32, Sets>,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Set { index, sets }) => {
            escalate_permissions().expect("Failed to escalate permissions");

            sudo2::escalate_if_needed()
                .or_else(|_| sudo2::doas())
                .or_else(|_| sudo2::pkexec())
                .expect("Failed to escalate privileges");

            let nvml = Nvml::init().expect("Failed to initialize NVML");

            let mut device = nvml.device_by_index(*index).expect("Failed to get GPU");

            sets.apply(&mut device);
            println!("Successfully set GPU parameters.");
        }
        Some(Commands::Get { index }) => {
            let nvml = Nvml::init().expect("Failed to initialize NVML");
            let device = nvml.device_by_index(*index).expect("Failed to get GPU");

            let freq_offset = device.gpc_clock_vf_offset();
            match freq_offset {
                Ok(freq_offset) => println!("GPU core clock offset: {} MHz", freq_offset),
                Err(e) => eprintln!("Failed to get GPU core clock offset: {:?}", e),
            }

            let mem_offset = device.mem_clock_vf_offset();
            match mem_offset {
                Ok(mem_offset) => println!("GPU memory clock offset: {} MHz", mem_offset),
                Err(e) => eprintln!("Failed to get GPU memory clock offset: {:?}", e),
            }

            let power_limit = device.enforced_power_limit();
            match power_limit {
                Ok(power_limit) => println!("GPU power limit: {} W", power_limit / 1000),
                Err(e) => eprintln!("Failed to get GPU power limit: {:?}", e),
            }

            let power_constraints = device.power_management_limit_constraints();
            match power_constraints {
                Ok(constraints) => println!(
                    "GPU power limit range: {}-{} W",
                    constraints.min_limit / 1000,
                    constraints.max_limit / 1000
                ),
                Err(e) => eprintln!("Failed to get GPU power limit constraints: {:?}", e),
            }

            // Target temperature (acoustic limit)
            match get_acoustic_temperature(&device) {
                Some(temp) => println!("Target temperature (acoustic): {}°C", temp),
                None => eprintln!("Failed to get target temperature (not supported or not set)"),
            }

            let (min_temp, max_temp) = get_acoustic_temperature_range(&device);
            match (min_temp, max_temp) {
                (Some(min), Some(max)) => {
                    println!("Target temperature range: {}°C - {}°C", min, max)
                }
                _ => eprintln!("Failed to get target temperature range (not supported)"),
            }
        }
        None => {
            let Ok(config_file) = std::fs::read_to_string(cli.file) else {
                panic!("Configuration file not found and no valid arguments were provided. Run `nvidia_oc --help` for more information.");
            };

            escalate_permissions().expect("Failed to escalate permissions");

            let config: Config =
                serde_json::from_str(&config_file).expect("Invalid configuration file");

            let nvml = Nvml::init().expect("Failed to initialize NVML");

            for (index, sets) in config.sets {
                let mut device = nvml.device_by_index(index).expect("Failed to get GPU");
                sets.apply(&mut device);
            }
            println!("Successfully set GPU parameters.");
        }
        Some(Commands::Completion { shell }) => {
            generate_completion_script(*shell);
        }
    }
}

fn escalate_permissions() -> Result<(), Box<dyn std::error::Error>> {
    if sudo2::running_as_root() {
        return Ok(());
    }

    if which::which("sudo").is_ok() {
        sudo2::escalate_if_needed()?;
    } else if which::which("doas").is_ok() {
        sudo2::doas()?;
    } else if which::which("pkexec").is_ok() {
        sudo2::pkexec()?;
    } else {
        return Err("Please install sudo, doas or pkexec and try again. Alternatively, run the program as root.".into());
    }

    Ok(())
}

fn generate_completion_script<G: Generator>(gen: G) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(gen, &mut cmd, name, &mut io::stdout());
}

/// Gets the raw NVML device handle from a Device.
/// This is needed to call low-level NVML functions not exposed by nvml-wrapper.
fn get_raw_device_handle(device: &Device) -> nvmlDevice_t {
    // SAFETY: Device stores the raw handle as the first field in its struct.
    // We access it by transmuting the reference.
    unsafe { std::ptr::read(device as *const Device as *const nvmlDevice_t) }
}

/// Sets the acoustic (target) temperature threshold.
/// The GPU will automatically adjust fan curves to maintain this temperature.
fn set_acoustic_temperature(device: &Device, temp_celsius: u32) -> Result<(), String> {
    let handle = get_raw_device_handle(device);
    let mut temp = temp_celsius as i32;

    // Load the NVML library
    let nvml_lib = unsafe {
        NvmlLib::new("libnvidia-ml.so.1")
            .or_else(|_| NvmlLib::new("libnvidia-ml.so"))
            .map_err(|e| format!("Failed to load NVML library: {:?}", e))?
    };

    let result = unsafe {
        nvml_lib.nvmlDeviceSetTemperatureThreshold(
            handle,
            nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR,
            &mut temp,
        )
    };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Ok(())
    } else {
        Err(format!("NVML error code: {}", result))
    }
}

/// Gets the current acoustic (target) temperature threshold.
fn get_acoustic_temperature(device: &Device) -> Option<u32> {
    let handle = get_raw_device_handle(device);
    let mut temp: u32 = 0;

    let nvml_lib = unsafe {
        NvmlLib::new("libnvidia-ml.so.1")
            .or_else(|_| NvmlLib::new("libnvidia-ml.so"))
            .ok()?
    };

    let result = unsafe {
        nvml_lib.nvmlDeviceGetTemperatureThreshold(
            handle,
            nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_CURR,
            &mut temp,
        )
    };

    if result == nvmlReturn_enum_NVML_SUCCESS {
        Some(temp)
    } else {
        None
    }
}

/// Gets the min and max acoustic temperature range.
fn get_acoustic_temperature_range(device: &Device) -> (Option<u32>, Option<u32>) {
    let handle = get_raw_device_handle(device);
    let mut min_temp: u32 = 0;
    let mut max_temp: u32 = 0;

    let nvml_lib = match unsafe {
        NvmlLib::new("libnvidia-ml.so.1").or_else(|_| NvmlLib::new("libnvidia-ml.so"))
    } {
        Ok(lib) => lib,
        Err(_) => return (None, None),
    };

    let min_result = unsafe {
        nvml_lib.nvmlDeviceGetTemperatureThreshold(
            handle,
            nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MIN,
            &mut min_temp,
        )
    };

    let max_result = unsafe {
        nvml_lib.nvmlDeviceGetTemperatureThreshold(
            handle,
            nvmlTemperatureThresholds_enum_NVML_TEMPERATURE_THRESHOLD_ACOUSTIC_MAX,
            &mut max_temp,
        )
    };

    let min = if min_result == nvmlReturn_enum_NVML_SUCCESS {
        Some(min_temp)
    } else {
        None
    };

    let max = if max_result == nvmlReturn_enum_NVML_SUCCESS {
        Some(max_temp)
    } else {
        None
    };

    (min, max)
}
