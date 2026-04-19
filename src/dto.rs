use std::fmt;

pub struct SwapResult {
    pub symbol: String,
    pub amount: u64,
    pub dex: String,
}

impl fmt::Display for SwapResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[OK] {} | {} SOL -> {}",
            self.symbol, self.amount, self.dex
        )
    }
}

pub struct SwapError {
    pub symbol: String,
    pub dex: String,
    pub reason: String,
}

impl fmt::Display for SwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[FAIL] {} | {} failed: {}",
            self.symbol, self.dex, self.reason
        )
    }
}
