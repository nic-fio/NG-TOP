// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Buffer circolare a capacità fissa per serie di valori 0..1.

use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct History {
    cap: usize,
    clamp: bool,
    buf: VecDeque<f64>,
}

impl History {
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        History {
            cap,
            clamp: true,
            buf: VecDeque::with_capacity(cap),
        }
    }

    pub fn push(&mut self, v: f64) {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        let v = if !v.is_finite() {
            0.0
        } else if self.clamp {
            v.clamp(0.0, 1.0)
        } else {
            v.max(0.0)
        };
        self.buf.push_back(v);
    }

    /// Serie non normalizzata (valori >= 0 di qualsiasi scala, es. byte/s).
    pub fn raw(cap: usize) -> Self {
        History {
            clamp: false,
            ..History::new(cap)
        }
    }

    /// Sostituisce tutto il contenuto con `vals` (rispettando limiti e capacità).
    pub fn replace(&mut self, vals: impl IntoIterator<Item = f64>) {
        self.buf.clear();
        for v in vals {
            self.push(v);
        }
    }

    /// Massimo tra gli ultimi `n` campioni.
    pub fn max_last(&self, n: usize) -> f64 {
        self.last_n(n).into_iter().fold(0.0, f64::max)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Gli ultimi `n` campioni, dal più vecchio al più recente.
    pub fn last_n(&self, n: usize) -> Vec<f64> {
        let skip = self.buf.len().saturating_sub(n);
        self.buf.iter().skip(skip).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serie_grezza() {
        let mut h = History::raw(4);
        h.push(5e6);
        h.push(-1.0);
        h.push(2.0);
        assert_eq!(h.last_n(3), [5e6, 0.0, 2.0]);
        assert_eq!(h.max_last(2), 2.0);
        assert_eq!(h.max_last(9), 5e6);
    }

    #[test]
    fn circolare() {
        let mut h = History::new(3);
        assert!(h.is_empty());
        for v in [0.1, 0.2, 0.3, 0.4] {
            h.push(v);
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.last_n(10), [0.2, 0.3, 0.4]);
        assert_eq!(h.last_n(2), [0.3, 0.4]);
        h.push(f64::NAN);
        h.push(7.0);
        assert_eq!(h.last_n(2), [0.0, 1.0]);
    }
}
