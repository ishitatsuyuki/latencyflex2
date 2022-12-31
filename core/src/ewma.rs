pub struct EwmaEstimator {
    current: f64,
    current_weight: f64,
    alpha: f64,
}

impl EwmaEstimator {
    pub fn new(alpha: f64) -> EwmaEstimator {
        EwmaEstimator {
            current: 0.,
            current_weight: 0.,
            alpha,
        }
    }

    pub fn update(&mut self, v: f64) {
        self.current = (1. - self.alpha) * self.current + self.alpha * v;
        self.current_weight = (1. - self.alpha) * self.current_weight + self.alpha;
    }

    pub fn get(&self) -> f64 {
        if self.current_weight == 0. {
            0.
        } else {
            self.current / self.current_weight
        }
    }
}
