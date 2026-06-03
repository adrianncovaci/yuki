pub fn decode(code: u32) -> Option<&'static str> {
    let msg = match code {
        6000 => "Not approved",
        6001 => "invalid update amm config flag",
        6002 => "Account lack",
        6003 => {
            "Remove liquidity, collect fees owed and reward then you can close position account"
        }
        6004 => "Tick out of range",
        6005 => "The lower tick must be below the upper tick",
        6006 => "The tick must be greater, or equal to the minimum tick(-443636)",
        6007 => "The tick must be lesser than, or equal to the maximum tick(443636)",
        6008 => "tick % tick_spacing must be zero",
        6009 => "Invalid tick array account",
        6010 => "Invalid tick array boundary",
        6011 => "Square root price limit overflow",
        6012 => "sqrt_price_x64 out of range",
        6013 => "Liquidity sub delta L must be smaller than before",
        6014 => "Liquidity add delta L must be greater, or equal to before",
        6015 => "Both token amount must not be zero while supply liquidity",
        6016 => "Liquidity insufficient",
        6017 => "Price slippage check",
        6018 => "Too little output received",
        6019 => "Too much input paid",
        6020 => "Swap special amount can not be zero",
        6021 => "Input pool vault is invalid",
        6022 => "Swap input or output amount is too small",
        6023 => "Not enough tick array account",
        6024 => "Invalid first tick array account",
        6025 => "Invalid reward index",
        6026 => "The init reward token reach to the max",
        6027 => "The init reward token already in use",
        6028 => "The reward tokens must contain one of pool vault mint except the last reward",
        6029 => "Invalid reward init param",
        6030 => "Invalid collect reward input account number",
        6031 => "Invalid reward period",
        6032 => {
            "Modification of emissions is allowed within 72 hours from the end of the previous cycle"
        }
        6033 => "uninitialized reward info",
        6034 => "Not support token_2022 mint extension",
        6035 => "Missing tickarray bitmap extension account",
        6036 => "Insufficient liquidity for this direction",
        6037 => "Max token overflow",
        6038 => "Calculate overflow",
        6039 => "TransferFee calculate not match",
        6040 => "Order already fully filled, cannot modify",
        6041 => "Invalid order phase",
        6042 => "Invalid limit order amount",
        6043 => "Tick order phase saturated",
        6044 => "Invalid dynamic fee config params",
        6045 => "Invalid fee on which token (must be 0, 1, or 2)",
        6046 => "sqrt_price_x64 must be greater than 0",
        6047 => "liquidity must be greater than 0",
        6048 => "base_flag is required when liquidity is zero",
        6049 => "Mint account is required but not provided",
        6050 => "Token-2022 program is required but not provided",
        _ => return None,
    };
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn decodes_known_codes() {
        assert_eq!(decode(6000), Some("Not approved"));
        assert_eq!(decode(6018), Some("Too little output received"));
        assert_eq!(
            decode(6050),
            Some("Token-2022 program is required but not provided")
        );
    }

    #[test]
    fn unknown_codes_return_none() {
        assert_eq!(decode(0), None);
        assert_eq!(decode(5999), None);
        assert_eq!(decode(6051), None);
    }
}
