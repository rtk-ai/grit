use std::f64::consts::PI;

/// Format a floating point value to a specific number of decimal places
pub fn to_decimal_string(value: f64, digits: u32) -> String {
    let multiplier = 10_f64.powi(digits as i32);
    let rounded = (value * multiplier).round() / multiplier;
    format!("{:.prec$}", rounded, prec = digits as usize)
}

/// Compare a calculated value against the known value of pi, returning absolute error
pub fn compare_with_known(calculated: f64) -> f64 {
    let error = (calculated - PI).abs();
    error
}

/// Count how many significant digits match the known value of pi
pub fn significant_digits(calculated: f64) -> u32 {
    let known = format!("{:.15}", PI);
    let calc = format!("{:.15}", calculated);

    let known_chars: Vec<char> = known.chars().collect();
    let calc_chars: Vec<char> = calc.chars().collect();

    let mut correct = 0_u32;
    for i in 0..known_chars.len().min(calc_chars.len()) {
        if known_chars[i] == calc_chars[i] {
            if known_chars[i] != '.' {
                correct += 1;
            }
        } else {
            break;
        }
    }

    correct
}

/// Format a value in scientific notation with explicit exponent
pub fn format_scientific(value: f64) -> String {
    if value == 0.0 {
        return "0.0e0".to_string();
    }

    let exponent = value.abs().log10().floor() as i32;
    let mantissa = value / 10_f64.powi(exponent);

    format!("{:.6}e{}", mantissa, exponent)
}

/// Compute a theoretical error bound for a given algorithm and iteration count
pub fn error_bound(algo: &str, iterations: u64) -> f64 {
    let n = iterations as f64;

    match algo {
        "leibniz" => 1.0 / (2.0 * n + 1.0),
        "monte_carlo" => 4.0 / n.sqrt(),
        "nilakantha" => 4.0 / ((2.0 * n + 2.0) * (2.0 * n + 3.0) * (2.0 * n + 4.0)),
        "chudnovsky" => 10_f64.powf(-14.0 * n),
        "wallis" => PI / (2.0 * n),
        "ramanujan" => 10_f64.powf(-8.0 * n),
        "bbp" => 16_f64.powf(-n),
        "gauss_legendre" => 10_f64.powf(-(2_f64.powi(n as i32))),
        _ => f64::NAN,
    }
}
