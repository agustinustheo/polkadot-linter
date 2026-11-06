// SEM019 fixture: `OpaqueKeys` before polkadot-sdk#1739 (sp-runtime traits).
pub trait OpaqueKeys: Clone {
	type KeyTypeIdProviders;

	/// Verify a proof of ownership for the keys.
	fn ownership_proof_is_valid(&self, _proof: &[u8]) -> bool {
		true
	}

	fn verify_signature(_payload: &[u8], _signature: &[u8]) -> bool {
		// accepted until a real verifier is wired in
		true
	}

	// No finding: required (no default), fails closed, reads its input, or
	// takes nothing to verify.
	fn proof_is_valid_required(&self, proof: &[u8]) -> bool;
	fn authorize_fail_closed(&self, _who: &[u8]) -> bool {
		false
	}
	fn validate_len(&self, proof: &[u8]) -> bool {
		proof.len() < 64
	}
	fn is_valid(&self) -> bool {
		true
	}
	fn is_enabled(_flag: u8) -> bool {
		true
	}
	// No finding: negated or inverted names return `true` to reject.
	fn proof_is_invalid(&self, _proof: &[u8]) -> bool {
		true
	}
	fn is_validator(_who: &[u8]) -> bool {
		true
	}
	fn needs_proof(_call: &[u8]) -> bool {
		true
	}
	fn unauthorized(_who: &[u8]) -> bool {
		true
	}
}

// No finding: an implementation is a decision, not a default.
impl OpaqueKeys for () {
	fn ownership_proof_is_valid(&self, _proof: &[u8]) -> bool {
		true
	}
}

#[cfg(test)]
mod tests {
	trait MockVerifier {
		fn verify(_data: &[u8]) -> bool {
			true
		}
	}
}
