use std::fmt;

pub struct SwapResult {
    pub symbol: String,
    pub amount: u64,
    pub dex: String,
    pub estimated_output: f64,
    pub signature: String,
}

impl fmt::Display for SwapResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[OK] {} | {:.4} SOL -> {} | est. {:.4}\n{}",
            self.symbol,
            self.amount as f64 / 1e9,
            self.dex,
            self.estimated_output,
            self.signature
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
