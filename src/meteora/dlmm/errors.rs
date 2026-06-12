/// Decode a Meteora DLMM (LB CLMM) custom program error code into its message.
/// Codes mirror `meteora_sol::errors::LbClmmError` (anchor offset 6000 = 0x1770).
/// Only the swap-relevant subset is listed; unknown codes return `None`.
pub fn decode(code: u32) -> Option<&'static str> {
    let msg = match code {
        6000 => "Invalid start bin index",
        6001 => "Invalid bin id",
        6002 => "Invalid input data",
        6003 => "Exceeded amount slippage tolerance",
        6004 => "Exceeded bin slippage tolerance",
        6007 => "Zero liquidity",
        6009 => "Bin array not found",
        6010 => "Invalid token mint",
        6012 => "Pair insufficient liquidity",
        6018 => "Math operation overflow",
        6027 => "Invalid bin array",
        6028 => "Bin arrays must be continuous",
        6036 => "Bitmap extension account is not provided",
        6037 => "Cannot find non-zero liquidity binArrayId",
        6038 => "Bin id out of bound",
        6039 => "Insufficient out amount",
        _ => return None,
    };
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_known_codes() {
        assert_eq!(decode(6003), Some("Exceeded amount slippage tolerance"));
        assert_eq!(decode(6009), Some("Bin array not found"));
    }

    #[test]
    fn unknown_codes_return_none() {
        assert_eq!(decode(1), None);
        assert_eq!(decode(6005), None);
    }
}
