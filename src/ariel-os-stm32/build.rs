use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    // handle CONFIG_SWI
    {
        let dest_path = Path::new(&out_dir).join("swi.rs");
        if let Ok(var) = env::var("CONFIG_SWI") {
            fs::write(
                &dest_path,
                format!("ariel_os_embassy_common::executor_swi!({var});\n").as_bytes(),
            )
            .expect("write failed");
        } else {
            fs::write(
                &dest_path,
                b"compile_error!(\"swi.rs included but CONFIG_SWI not set!\");\n",
            )
            .expect("write failed");
        }

        println!("cargo::rerun-if-env-changed=CONFIG_SWI");
    }

    peripheral_cfg_from_metapac();
    exti_irq_bindings_from_metapac();
    gpio_outputs_from_env();
}

// Configure initial board outputs without taking pins away from applications.
fn gpio_outputs_from_env() {
    use std::fmt::Write as _;

    println!("cargo::rerun-if-env-changed=CONFIG_GPIO_OUTPUTS");
    let config = env::var("CONFIG_GPIO_OUTPUTS").unwrap_or_default();
    let mut code = String::from("fn init_gpio_outputs(_peripherals: &mut OptionalPeripherals) {\n");
    let mut seen = std::collections::HashSet::new();

    for entry in config.split(',').filter(|entry| !entry.is_empty()) {
        let (pin, level) = entry
            .split_once(':')
            .expect("CONFIG_GPIO_OUTPUTS requires PIN:low or PIN:high entries");
        let pin_bytes = pin.as_bytes();
        assert!(
            (3..=4).contains(&pin_bytes.len())
                && pin_bytes[0] == b'P'
                && pin_bytes[1].is_ascii_uppercase()
                && pin_bytes[2..].iter().all(u8::is_ascii_digit)
                && pin[2..].parse::<u8>().is_ok_and(|number| number < 16),
            "invalid GPIO pin in CONFIG_GPIO_OUTPUTS: {pin}"
        );
        assert!(seen.insert(pin), "duplicate GPIO pin: {pin}");
        let level = match level {
            "low" => "Low",
            "high" => "High",
            _ => panic!("invalid GPIO level in CONFIG_GPIO_OUTPUTS: {level}"),
        };
        writeln!(
            code,
            "{{ let output = embassy_stm32::gpio::Output::new(\n\
             _peripherals.{pin}.as_mut().unwrap().reborrow(),\n\
             embassy_stm32::gpio::Level::{level}, embassy_stm32::gpio::Speed::Low);\n\
             core::mem::forget(output); }}"
        )
        .unwrap();
    }
    code.push_str("}\n");
    let out_dir = env::var_os("OUT_DIR").unwrap();
    fs::write(Path::new(&out_dir).join("gpio_outputs.rs"), code).expect("write failed");
}

/// Generates `for_each_exti_interrupt!()` with all EXTI interrupt vectors of the selected chip, so
/// they can be bound to embassy's EXTI handler.
///
/// embassy-stm32 >= 0.5 enables all EXTI interrupts in `exti::init()` but requires the user to
/// bind them via `bind_interrupts!()`, which needs the chip-specific vector names.
/// The vectors are taken from the EXTI peripheral's line -> interrupt mapping (lines 0..=15),
/// which is what embassy-stm32 uses for `exti::Channel::IRQ`.
///
/// # Panics
///
/// Panics if the output file cannot be written.
fn exti_irq_bindings_from_metapac() {
    let mut irqs: Vec<String> = Vec::new();
    for p in stm32_metapac::metadata::METADATA.peripherals {
        if p.name != "EXTI" {
            continue;
        }
        for i in p.interrupts {
            let line = i
                .signal
                .strip_prefix("EXTI")
                .and_then(|n| n.parse::<u8>().ok());
            if line.is_some_and(|n| n < 16) {
                irqs.push(i.interrupt.to_ascii_uppercase());
            }
        }
    }
    // Like embassy-stm32's build.rs: the PAC doesn't list `EXTI2_TSC` as the IRQ of EXTI2.
    if stm32_metapac::metadata::METADATA
        .interrupts
        .iter()
        .any(|i| i.name == "EXTI2_TSC")
    {
        irqs.push("EXTI2_TSC".to_string());
    }
    irqs.sort();
    irqs.dedup();

    // Generates `for_each_exti_interrupt!($m)`, which invokes `$m!(IRQ0, IRQ1, ...)` with all EXTI
    // interrupt vectors of the chip.
    let out = format!(
        "macro_rules! for_each_exti_interrupt {{\n    ($m:ident) => {{\n        $m!({});\n    }};\n}}\n",
        irqs.join(", ")
    );

    let out_dir = std::env::var_os("OUT_DIR").unwrap();
    std::fs::write(Path::new(&out_dir).join("exti_irqs.rs"), out).expect("write failed");
}

// Enable peripheral `cfg` flags, data taken from `stm32-metapac`.
// Similar to https://github.com/embassy-rs/embassy/blob/ef32187ed7349f3883d997b6f1590e11dbc8db81/embassy-stm32/build.rs#L38-L43
fn peripheral_cfg_from_metapac() {
    use std::collections::HashSet;
    fn cfg_only_once(seen: &mut HashSet<String>, cfg: &str) {
        if seen.insert(cfg.to_string()) {
            println!("cargo::rustc-cfg={cfg}");
        }
    }
    let mut seen = HashSet::new();
    for p in stm32_metapac::metadata::METADATA.peripherals {
        if let Some(r) = &p.registers {
            cfg_only_once(&mut seen, r.kind);
            cfg_only_once(&mut seen, &format!("{}_{}", r.kind, r.version));
        }
    }
}
