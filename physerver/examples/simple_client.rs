use std::time::Duration;

/// Simple example client that uses IPC to communicate with physerver
///
/// Run this after starting physerver:
///   cargo run --example simple_client
use physerver::{Command, CommandFlags, IpcClient};

fn main() -> anyhow::Result<()> {
    println!("PhyCMD Simple Client Example");
    println!("Connecting to physerver via IPC...");

    // Connect to physerver's shared memory
    let client = IpcClient::connect()?;

    println!("Connected! Press Ctrl+C to exit.\n");

    // Blink GPIO pins in a pattern
    let mut pattern = 0u16;
    let mut count = 0;

    loop {
        // Create a command
        let mut cmd = Command::default();
        cmd.digital_out = pattern;
        cmd.dac[0] = (count % 4096) as u16; // Ramp DAC 0
        cmd.dac[1] = 4095 - (count % 4096) as u16; // Inverse ramp DAC 1

        cmd.flags = CommandFlags {
            adc_enable: true,
            dac_enable: true,
            pwm_enable: false,
            reset_seq: false,
            watchdog_disable: false,
        };

        // Send command to physerver
        client.write_command(&cmd);

        // Read status from physerver
        let status = client.read_status();

        // Print status
        println!("Status:");
        println!("  Digital In:  {:016b}", status.digital_in);
        println!("  Digital Out: {:016b}", status.digital_out);
        println!(
            "  ADC 0: {:4}  ADC 1: {:4}  ADC 2: {:4}  ADC 3: {:4}",
            status.adc[0], status.adc[1], status.adc[2], status.adc[3]
        );
        println!("  Loop Time: {} µs", status.loop_time_us);
        println!("  Uptime: {} ms", status.uptime_ms);
        println!("  Errors: {}", status.error_count);
        println!();

        // Rotate pattern
        pattern = pattern.rotate_left(1);
        if pattern == 0 {
            pattern = 1;
        }

        count += 10;

        std::thread::sleep(Duration::from_millis(500));
    }
}
