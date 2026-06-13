//! DSP primitives ported 1:1 from HTCommander's SSTV port of
//! <https://github.com/xdsopl/robot36> (C# sources under
//! `dev/scratch/benshi-re/HTCommander/src/SSTV/`).
//!
//! The C# originals are mutable reference classes with fluent in-place methods.
//! `Complex` is ported as a `Copy` value type with value-returning ops; the
//! stateful filters keep their ring-buffer / accumulator state in `&mut self`.

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    /// `Real*Real + Imag*Imag`
    pub fn norm(self) -> f32 {
        self.re * self.re + self.im * self.im
    }

    pub fn abs(self) -> f32 {
        self.norm().sqrt()
    }

    pub fn arg(self) -> f32 {
        self.im.atan2(self.re)
    }

    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }

    pub fn mul_scalar(self, v: f32) -> Self {
        Self {
            re: self.re * v,
            im: self.im * v,
        }
    }

    pub fn div_scalar(self, v: f32) -> Self {
        Self {
            re: self.re / v,
            im: self.im / v,
        }
    }

}

impl std::ops::Add for Complex {
    type Output = Complex;
    fn add(self, o: Complex) -> Complex {
        Complex {
            re: self.re + o.re,
            im: self.im + o.im,
        }
    }
}

impl std::ops::Mul for Complex {
    type Output = Complex;
    fn mul(self, other: Complex) -> Complex {
        Complex {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

/// Numerically controlled oscillator (`Phasor.cs`).
pub struct Phasor {
    value: Complex,
    delta: Complex,
}

impl Phasor {
    pub fn new(freq: f64, rate: f64) -> Self {
        let omega = 2.0 * PI * freq / rate;
        Self {
            value: Complex::new(1.0, 0.0),
            delta: Complex::new(omega.cos() as f32, omega.sin() as f32),
        }
    }

    /// `value = (value * delta) / |value * delta|`
    pub fn rotate(&mut self) -> Complex {
        self.value = self.value * self.delta;
        self.value = self.value.div_scalar(self.value.abs());
        self.value
    }
}

/// FM discriminator (`FrequencyModulation.cs`).
pub struct FrequencyModulation {
    prev: f32,
    scale: f32,
}

impl FrequencyModulation {
    pub fn new(bandwidth: f64, sample_rate: f64) -> Self {
        Self {
            prev: 0.0,
            scale: (sample_rate / (bandwidth * PI)) as f32,
        }
    }

    fn wrap(value: f32) -> f32 {
        let pi = PI as f32;
        let two_pi = 2.0 * pi;
        if value < -pi {
            value + two_pi
        } else if value > pi {
            value - two_pi
        } else {
            value
        }
    }

    pub fn demod(&mut self, input: Complex) -> f32 {
        let phase = input.arg();
        let delta = Self::wrap(phase - self.prev);
        self.prev = phase;
        self.scale * delta
    }
}

/// Complex FIR via ring buffer (`ComplexConvolution.cs`).
pub struct ComplexConvolution {
    pub taps: Vec<f32>,
    real: Vec<f32>,
    imag: Vec<f32>,
    pos: usize,
    length: usize,
}

impl ComplexConvolution {
    pub fn new(length: usize) -> Self {
        Self {
            taps: vec![0.0; length],
            real: vec![0.0; length],
            imag: vec![0.0; length],
            pos: 0,
            length,
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn push(&mut self, input: Complex) -> Complex {
        self.real[self.pos] = input.re;
        self.imag[self.pos] = input.im;
        self.pos += 1;
        if self.pos >= self.length {
            self.pos = 0;
        }
        let mut re = 0.0;
        let mut im = 0.0;
        for i in 0..self.length {
            re += self.taps[i] * self.real[self.pos];
            im += self.taps[i] * self.imag[self.pos];
            self.pos += 1;
            if self.pos >= self.length {
                self.pos = 0;
            }
        }
        Complex::new(re, im)
    }
}

/// Digital delay line (`Delay.cs`).
pub struct Delay {
    buf: Vec<f32>,
    pos: usize,
    length: usize,
}

impl Delay {
    pub fn new(length: usize) -> Self {
        Self {
            buf: vec![0.0; length],
            pos: 0,
            length,
        }
    }

    pub fn push(&mut self, input: f32) -> f32 {
        let tmp = self.buf[self.pos];
        self.buf[self.pos] = input;
        self.pos += 1;
        if self.pos >= self.length {
            self.pos = 0;
        }
        tmp
    }
}

/// Sliding-window sum over a binary tree of accumulators (`SimpleMovingSum.cs`).
pub struct SimpleMovingSum {
    tree: Vec<f32>,
    leaf: usize,
    pub length: usize,
}

impl SimpleMovingSum {
    pub fn new(length: usize) -> Self {
        Self {
            tree: vec![0.0; 2 * length],
            leaf: length,
            length,
        }
    }

    pub fn add(&mut self, input: f32) {
        self.tree[self.leaf] = input;
        let mut child = self.leaf;
        let mut parent = self.leaf / 2;
        while parent > 0 {
            self.tree[parent] = self.tree[child] + self.tree[child ^ 1];
            child = parent;
            parent /= 2;
        }
        self.leaf += 1;
        if self.leaf >= self.tree.len() {
            self.leaf = self.length;
        }
    }

    pub fn sum(&self) -> f32 {
        self.tree[1]
    }

    pub fn sum_of(&mut self, input: f32) -> f32 {
        self.add(input);
        self.sum()
    }
}

/// `SimpleMovingAverage`: sum / length.
pub struct SimpleMovingAverage {
    inner: SimpleMovingSum,
}

impl SimpleMovingAverage {
    pub fn new(length: usize) -> Self {
        Self {
            inner: SimpleMovingSum::new(length),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.length
    }

    pub fn is_empty(&self) -> bool {
        self.inner.length == 0
    }

    pub fn avg(&mut self, input: f32) -> f32 {
        self.inner.sum_of(input) / self.inner.length as f32
    }
}

/// Exponential moving average / one-pole low-pass (`ExponentialMovingAverage.cs`).
pub struct ExponentialMovingAverage {
    alpha: f32,
    prev: f32,
}

impl Default for ExponentialMovingAverage {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            prev: 0.0,
        }
    }
}

impl ExponentialMovingAverage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn avg(&mut self, input: f32) -> f32 {
        self.prev = self.prev * (1.0 - self.alpha) + self.alpha * input;
        self.prev
    }

    pub fn set_alpha(&mut self, alpha: f64) {
        self.alpha = alpha as f32;
    }

    pub fn set_alpha_order(&mut self, alpha: f64, order: i32) {
        self.set_alpha(alpha.powf(1.0 / order as f64));
    }

    pub fn cutoff_order(&mut self, freq: f64, rate: f64, order: i32) {
        let x = (2.0 * PI * freq / rate).cos();
        self.set_alpha_order(x - 1.0 + (x * (x - 4.0) + 3.0).sqrt(), order);
    }

    pub fn reset(&mut self) {
        self.prev = 0.0;
    }
}

/// Hysteresis comparator (`SchmittTrigger.cs`).
pub struct SchmittTrigger {
    low: f32,
    high: f32,
    previous: bool,
}

impl SchmittTrigger {
    pub fn new(low: f32, high: f32) -> Self {
        Self {
            low,
            high,
            previous: false,
        }
    }

    pub fn latch(&mut self, input: f32) -> bool {
        if self.previous {
            if input < self.low {
                self.previous = false;
            }
        } else if input > self.high {
            self.previous = true;
        }
        self.previous
    }
}

/// FIR design helpers (`Filter.cs`).
pub mod filter {
    use super::PI;

    pub fn sinc(mut x: f64) -> f64 {
        if x == 0.0 {
            return 1.0;
        }
        x *= PI;
        x.sin() / x
    }

    pub fn low_pass(cutoff: f64, rate: f64, n: usize, big_n: usize) -> f64 {
        let f = 2.0 * cutoff / rate;
        let x = n as f64 - (big_n as f64 - 1.0) / 2.0;
        f * sinc(f * x)
    }
}

/// Hann window (`Hann.cs`).
pub fn hann_window(n: usize, big_n: usize) -> f64 {
    0.5 * (1.0 - ((2.0 * PI * n as f64) / (big_n as f64 - 1.0)).cos())
}

/// Kaiser window via zeroth-order modified Bessel I0 (`Kaiser.cs`).
pub struct Kaiser;

impl Kaiser {
    fn i0(x: f64) -> f64 {
        // i0(x) converges within 35 terms for x in [-3pi, 3pi].
        let mut summands = [0.0f64; 35];
        summands[0] = 1.0;
        let mut val = 1.0;
        for (n, s) in summands.iter_mut().enumerate().skip(1) {
            val *= x / (2.0 * n as f64);
            *s = val * val;
        }
        // Sort ascending, sum from largest to smallest (matches C# Array.Sort + reverse loop).
        summands.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut sum = 0.0;
        for n in (0..summands.len()).rev() {
            sum += summands[n];
        }
        sum
    }

    pub fn window(a: f64, n: usize, big_n: usize) -> f64 {
        let t = (2.0 * n as f64) / (big_n as f64 - 1.0) - 1.0;
        Self::i0(PI * a * (1.0 - t * t).sqrt()) / Self::i0(PI * a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_mul_matches_definition() {
        // (1+2i)(3+4i) = 3+4i+6i+8i^2 = -5+10i
        let r = Complex::new(1.0, 2.0) * Complex::new(3.0, 4.0);
        assert!((r.re - -5.0).abs() < 1e-6);
        assert!((r.im - 10.0).abs() < 1e-6);
    }

    #[test]
    fn complex_arg_and_abs() {
        let c = Complex::new(0.0, 1.0);
        assert!((c.arg() - (PI as f32 / 2.0)).abs() < 1e-6);
        assert!((c.abs() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn moving_sum_windows_last_n() {
        // Window length 3: after pushing 1,2,3,4 the sum is 2+3+4 = 9.
        let mut s = SimpleMovingSum::new(3);
        for v in [1.0, 2.0, 3.0, 4.0] {
            s.add(v);
        }
        assert!((s.sum() - 9.0).abs() < 1e-5);
    }

    #[test]
    fn moving_average_divides_by_length() {
        let mut a = SimpleMovingAverage::new(4);
        let mut last = 0.0;
        for v in [4.0, 4.0, 4.0, 4.0] {
            last = a.avg(v);
        }
        assert!((last - 4.0).abs() < 1e-5);
    }

    #[test]
    fn schmitt_trigger_hysteresis() {
        let mut t = SchmittTrigger::new(-0.5, 0.5);
        assert!(!t.latch(0.0)); // starts low, 0 < high -> stays low
        assert!(t.latch(1.0)); // > high -> high
        assert!(t.latch(0.0)); // between -> stays high
        assert!(!t.latch(-1.0)); // < low -> low
    }

    #[test]
    fn phasor_stays_on_unit_circle() {
        let mut p = Phasor::new(1000.0, 32000.0);
        for _ in 0..5000 {
            let v = p.rotate();
            assert!((v.abs() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn fm_demod_recovers_constant_frequency() {
        // A complex tone at +bandwidth/2 should demod near +1 after warm-up.
        let rate = 32000.0;
        let bandwidth = 800.0;
        let freq = bandwidth / 2.0; // +400 Hz
        let mut osc = Phasor::new(freq, rate);
        let mut fm = FrequencyModulation::new(bandwidth, rate);
        let mut last = 0.0;
        for _ in 0..2000 {
            last = fm.demod(osc.rotate());
        }
        assert!((last - 1.0).abs() < 0.05, "demod={last}");
    }
}
