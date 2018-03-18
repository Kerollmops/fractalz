use num_complex::Complex64;
use fractal::Fractal;

pub struct Julia {
    c: Complex64,
}

impl Julia {
    pub fn new(x: f64, y: f64) -> Julia {
        Julia {
            c: Complex64::new(0.3 + x, 0.5 + y)
        }
    }
}

impl Fractal for Julia {
    fn iterations(&self, x: f64, y: f64) -> u8 {
        let mut iterations = 0;
        let mut z = Complex64::new(x, y);

        while (z + z).re <= 4.0 && iterations < u8::max_value() {
            z = z * z + self.c;
            iterations += 1;
        }

        iterations
    }
}