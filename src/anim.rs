use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnimParam {
    pub enabled: bool,
    pub speed: f32,  // cycles per second
    pub low: f32,
    pub high: f32,
}

impl AnimParam {
    pub fn new(current: f32, min: f32, max: f32) -> Self {
        let range = max - min;
        Self {
            enabled: false,
            speed: 0.5,
            low: (current - range * 0.25).max(min),
            high: (current + range * 0.25).min(max),
        }
    }

    pub fn eval(&self, time: f64) -> f32 {
        let phase = (time as f32 * self.speed * std::f32::consts::TAU).sin();
        let t = (phase + 1.0) * 0.5;
        self.low + t * (self.high - self.low)
    }
}

pub type StepAnims = HashMap<String, AnimParam>;
