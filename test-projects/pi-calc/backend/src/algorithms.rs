use rand::Rng;

/// Leibniz series: pi/4 = 1 - 1/3 + 1/5 - 1/7 + ...
pub fn leibniz(iterations: u64) -> f64 {
    let mut sum = 0.0_f64;
    for i in 0..iterations {
        let term = 1.0 / (2 * i + 1) as f64;
        if i % 2 == 0 {
            sum += term;
        } else {
            sum -= term;
        }
    }
    sum * 4.0
}

/// Monte Carlo method: ratio of points inside unit circle quadrant
pub fn monte_carlo(samples: u64) -> f64 {
    let mut rng = rand::thread_rng();
    let mut inside_circle = 0_u64;

    for _ in 0..samples {
        let x: f64 = rng.gen();
        let y: f64 = rng.gen();
        let distance = x * x + y * y;
        if distance <= 1.0 {
            inside_circle += 1;
        }
    }

    4.0 * (inside_circle as f64) / (samples as f64)
}

/// Nilakantha series: pi = 3 + 4/(2*3*4) - 4/(4*5*6) + 4/(6*7*8) - ...
pub fn nilakantha(iterations: u64) -> f64 {
    let mut sum = 3.0_f64;
    for i in 0..iterations {
        let n = (2 * i + 2) as f64;
        let term = 4.0 / (n * (n + 1.0) * (n + 2.0));
        if i % 2 == 0 {
            sum += term;
        } else {
            sum -= term;
        }
    }
    sum
}

/// Chudnovsky algorithm: extremely fast convergence (~14 digits per term)
pub fn chudnovsky(iterations: u64) -> f64 {
    let mut sum = 0.0_f64;
    let mut factorial_6k = 1.0_f64;
    let mut factorial_3k = 1.0_f64;
    let mut factorial_k_cubed = 1.0_f64;
    let base = -262537412640768000.0_f64;

    for k in 0..iterations {
        if k > 0 {
            factorial_6k *= ((6 * k - 5) * (6 * k - 4) * (6 * k - 3) * (6 * k - 2) * (6 * k - 1) * (6 * k)) as f64;
            factorial_3k *= ((3 * k - 2) * (3 * k - 1) * (3 * k)) as f64;
            factorial_k_cubed *= (k * k * k) as f64;
        }

        let numerator = factorial_6k * (13591409.0 + 545140134.0 * k as f64);
        let denominator = factorial_3k * factorial_k_cubed * base.powi(k as i32);
        sum += numerator / denominator;
    }

    1.0 / (sum * 12.0 / (640320.0_f64).powf(1.5))
}

/// Wallis product: pi/2 = (2/1)(2/3)(4/3)(4/5)(6/5)(6/7)...
pub fn wallis(iterations: u64) -> f64 {
    let mut product = 1.0_f64;
    for i in 1..=iterations {
        let n = (2 * i) as f64;
        product *= n / (n - 1.0);
        product *= n / (n + 1.0);
    }
    product * 2.0
}

/// Ramanujan's formula: 1/pi = (2*sqrt(2)/9801) * sum(...)
pub fn ramanujan(iterations: u64) -> f64 {
    let mut sum = 0.0_f64;
    let mut factorial_4k = 1.0_f64;
    let mut factorial_k = 1.0_f64;
    let constant = 2.0 * 2.0_f64.sqrt() / 9801.0;

    for k in 0..iterations {
        if k > 0 {
            factorial_4k *= ((4 * k - 3) * (4 * k - 2) * (4 * k - 1) * (4 * k)) as f64;
            factorial_k *= k as f64;
        }

        let numerator = factorial_4k * (1103.0 + 26390.0 * k as f64);
        let denominator = factorial_k.powi(4) * 396.0_f64.powi(4 * k as i32);
        sum += numerator / denominator;
    }

    1.0 / (constant * sum)
}

/// Bailey-Borwein-Plouffe formula: allows extraction of individual hex digits
pub fn bbp(iterations: u64) -> f64 {
    let mut sum = 0.0_f64;

    for k in 0..iterations {
        let kf = k as f64;
        let base = 16.0_f64.powi(-(k as i32));
        let term = 4.0 / (8.0 * kf + 1.0)
            - 2.0 / (8.0 * kf + 4.0)
            - 1.0 / (8.0 * kf + 5.0)
            - 1.0 / (8.0 * kf + 6.0);
        sum += base * term;
    }

    sum
}

/// Gauss-Legendre algorithm: quadratic convergence (doubles correct digits each iteration)
pub fn gauss_legendre(iterations: u64) -> f64 {
    let mut a = 1.0_f64;
    let mut b = 1.0_f64 / 2.0_f64.sqrt();
    let mut t = 0.25_f64;
    let mut p = 1.0_f64;

    for _ in 0..iterations {
        let a_next = (a + b) / 2.0;
        let b_next = (a * b).sqrt();
        let t_next = t - p * (a - a_next).powi(2);
        let p_next = 2.0 * p;

        a = a_next;
        b = b_next;
        t = t_next;
        p = p_next;
    }

    (a + b).powi(2) / (4.0 * t)
}
