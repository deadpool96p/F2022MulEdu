//! Well-known EA Blaze union shapes.
//!
//! EA's TDF unions carry a *case index* that selects which member of the
//! union is active, plus a 3-byte member tag naming the field that carries
//! the value. Getting the case index wrong makes the client read the wrong
//! struct entirely, so the well-known shapes live here rather than being
//! spelled out at each call site.
//!
//! The two shapes we need:
//!
//! | union                | case | member   | fields                |
//! | -------------------- | ---- | -------- | --------------------- |
//! | `ServerAddress`      | 0    | `VALU`   | `HOST`, `IP`, `PORT`  |
//! | `NetworkAddress`     | 2    | `VALU`   | `EXIP`, `INIP`        |
//! | `NetworkAddress`     | 3    | `VALU`   | `IP`, `PORT`, `HOST`  |
//!
//! Sources, all independent of each other:
//!
//! * the byte-exact authentic EA `Redirector::getServerInstance` reply,
//!   whose `ADDR` is case 0 holding `HOST`/`IP`/`PORT`,
//! * the byte-exact authentic EA `UserSessions` frame, whose `ADDR` is
//!   case 2 holding `EXIP`/`INIP`,
//! * the reference repo's generated TDF definitions, which give
//!   `ServerAddress` case 0 and `NetworkAddress` cases 2 and 3.

use blaze_tdf::{TdfStruct, TdfUnion, TdfValue};

/// The `ServerAddress` union case index.
pub const SERVER_ADDRESS_CASE: u8 = 0;

/// `NetworkAddress::IpPairAddress` -- the case used by actual players.
pub const NETWORK_ADDRESS_IP_PAIR_CASE: u8 = 2;

/// `NetworkAddress::IpAddress` -- the case used by Dirtycast relay servers.
pub const NETWORK_ADDRESS_IP_CASE: u8 = 3;

/// Builds an `IpAddress` struct: `HOST` (optional), `IP` and `PORT`.
///
/// `ip` is the 32-bit address in network byte order, as EA stores it.
pub fn ip_address(host: &str, ip: u32, port: u16) -> TdfStruct {
    let mut addr = TdfStruct::new();
    if !host.is_empty() {
        addr.set("HOST", TdfValue::String(host.to_string()));
    }
    addr.set("IP", TdfValue::Integer(ip as u64));
    addr.set("PORT", TdfValue::Integer(port as u64));
    addr
}

/// Builds an `IpPairAddress` struct: `EXIP` and `INIP`.
pub fn ip_pair_address(external: TdfStruct, internal: TdfStruct) -> TdfStruct {
    let mut pair = TdfStruct::new();
    pair.set("EXIP", TdfValue::Struct(external));
    pair.set("INIP", TdfValue::Struct(internal));
    pair
}

/// Wraps a struct in the `ServerAddress` union (case 0).
pub fn server_address(addr: TdfStruct) -> TdfValue {
    TdfValue::Union(TdfUnion::with_member(
        SERVER_ADDRESS_CASE,
        TdfUnion::DEFAULT_MEMBER_TAG,
        TdfValue::Struct(addr),
    ))
}

/// Wraps a struct in the `NetworkAddress` union (case 2, `IpPairAddress`).
pub fn network_address_ip_pair(pair: TdfStruct) -> TdfValue {
    TdfValue::Union(TdfUnion::with_member(
        NETWORK_ADDRESS_IP_PAIR_CASE,
        TdfUnion::DEFAULT_MEMBER_TAG,
        TdfValue::Struct(pair),
    ))
}

/// Wraps a struct in the `NetworkAddress` union (case 3, `IpAddress`).
pub fn network_address_ip(addr: TdfStruct) -> TdfValue {
    TdfValue::Union(TdfUnion::with_member(
        NETWORK_ADDRESS_IP_CASE,
        TdfUnion::DEFAULT_MEMBER_TAG,
        TdfValue::Struct(addr),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_address_uses_case_zero() {
        let value = server_address(ip_address("", 0, 10013));
        match value {
            TdfValue::Union(u) => {
                assert_eq!(u.active_tag, 0);
                assert_eq!(u.member_tag, "VALU");
            }
            other => panic!("expected a union, got {:?}", other),
        }
    }

    #[test]
    fn network_address_uses_case_two_for_an_ip_pair() {
        let value = network_address_ip_pair(ip_pair_address(
            ip_address("", 0, 0),
            ip_address("", 0xC0A8_0107, 3659),
        ));
        match value {
            TdfValue::Union(u) => {
                assert_eq!(u.active_tag, 2);
                assert_eq!(u.member_tag, "VALU");
            }
            other => panic!("expected a union, got {:?}", other),
        }
    }
}
