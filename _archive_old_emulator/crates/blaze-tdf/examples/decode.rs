//! Decodes a Blaze frame or a bare TDF payload and prints it as a tree.
//!
//! This is the tool for reading what the client actually sent. Hand-decoding
//! Heat2 tags is error-prone -- the tag packing is four ASCII characters into
//! three bytes and the varsize integers have a non-obvious bit layout -- so
//! every captured payload is decoded with the same code the server uses.
//!
//! ```text
//! cargo run -p blaze-tdf --example decode -- "00 00 00 09 00 00 ..."
//! ```
//!
//! A 16-byte-or-longer input whose first four bytes look like a Fire2
//! payload length is treated as a full Fire2 frame: the header is printed and
//! only the payload is decoded. Anything shorter is decoded as a bare TDF
//! payload.

use std::io::Cursor;

use blaze_tdf::{TdfReader, TdfStruct, TdfValue};

/// Decodes `bytes` and reports how many bytes the struct actually consumed.
///
/// The consumed count is what tells a root struct apart from a nested one: a
/// nested struct is terminated by its own `0x00`, but a root struct is
/// delimited by the frame's `payloadSize` and may or may not carry a trailing
/// terminator. A decoder that stops early leaves bytes behind, and a decoder
/// that stops at the right place consumes them all.
fn decode_reporting_consumption(bytes: &[u8]) -> (TdfStruct, usize, usize) {
    let mut reader = TdfReader::new(Cursor::new(bytes));
    let struct_ = reader.read_struct().expect("payload must decode");
    let consumed = reader.into_inner().position() as usize;
    (struct_, consumed, bytes.len())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: decode <hex bytes...>");
        eprintln!("       decode --payload <hex bytes...>");
        std::process::exit(2);
    }

    let mut payload_only = false;
    let mut hex_parts: Vec<&str> = Vec::new();
    for arg in &args {
        if arg == "--payload" {
            payload_only = true;
        } else {
            hex_parts.extend(arg.split_whitespace());
        }
    }

    let mut bytes = Vec::with_capacity(hex_parts.len());
    for part in &hex_parts {
        let byte = u8::from_str_radix(part.trim_start_matches("0x"), 16)
            .unwrap_or_else(|_| panic!("not a hex byte: {part}"));
        bytes.push(byte);
    }

    if !payload_only && bytes.len() >= 16 {
        let payload_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let metadata_len = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let component = u16::from_be_bytes([bytes[6], bytes[7]]);
        let command = u16::from_be_bytes([bytes[8], bytes[9]]);
        let message_number = ((bytes[10] as u32) << 16) | ((bytes[11] as u32) << 8) | bytes[12] as u32;
        let message_type = bytes[13] >> 5;
        let user_index = bytes[13] & 0x1F;

        println!("Fire2 frame");
        println!("  payloadSize    = {payload_len}");
        println!("  metadataSize   = {metadata_len}");
        println!("  componentID    = 0x{component:04X}");
        println!("  commandID      = 0x{command:04X}");
        println!("  messageNumber  = {message_number}");
        println!("  messageType    = {message_type}");
        println!("  userIndex      = {user_index}");
        println!("  bytes[14..16]  = {:02X?}", &bytes[14..16]);
        println!();

        let start = 16 + metadata_len;
        let end = (start + payload_len).min(bytes.len());
        if start >= bytes.len() {
            println!("  <no payload>");
            return;
        }
        match blaze_tdf::TdfReader::new(Cursor::new(&bytes[start..end])).read_struct() {
            Ok(s) => print_tree(&s),
            Err(e) => eprintln!("decode failed: {e}"),
        }
        return;
    }

    let (s, consumed, total) = decode_reporting_consumption(&bytes);
    print_tree(&s);
    println!();
    println!("consumed {consumed} of {total} bytes");
    if consumed != total {
        println!(
            "  -> {} trailing byte(s) were NOT consumed",
            total - consumed
        );
    }
}

/// Prints a decoded struct as an indented tree.
fn print_tree(s: &TdfStruct) {
    for (tag, value) in &s.fields {
        print_value(tag, value, 0);
    }
}

/// Prints one value, recursing into structs, lists and maps.
fn print_value(tag: &str, value: &TdfValue, depth: usize) {
    let indent = "  ".repeat(depth);
    match value {
        TdfValue::Struct(inner) => {
            println!("{indent}{tag} (struct)");
            for (t, v) in &inner.fields {
                print_value(t, v, depth + 1);
            }
        }
        TdfValue::List(list) => {
            println!("{indent}{tag} (list of {} x {})", list.items.len(), type_name(list.item_type));
            for (i, item) in list.items.iter().enumerate() {
                print_value(&format!("[{i}]"), item, depth + 1);
            }
        }
        TdfValue::Map(map) => {
            println!(
                "{indent}{tag} (map, {} x {})",
                map.entries.len(),
                type_name(map.key_type)
            );
            for (k, v) in &map.entries {
                print_value(&format!("{k:?}"), v, depth + 1);
            }
        }
        TdfValue::Union(u) => {
            println!("{indent}{tag} (union case {})", u.active_tag);
            if let Some(inner) = &u.value {
                print_value(&u.member_tag, inner, depth + 1);
            }
        }
        TdfValue::Variable(v) => {
            println!("{indent}{tag} (variable, tdf_id {})", v.tdf_id);
            if let Some(inner) = &v.value {
                print_value(&v.member_tag, inner, depth + 1);
            }
        }
        TdfValue::String(s) => println!("{indent}{tag} = {s:?}"),
        TdfValue::Blob(b) => println!("{indent}{tag} = blob({} bytes)", b.len()),
        other => println!("{indent}{tag} = {other:?}"),
    }
}

/// The name of a wire type, for display.
fn type_name(t: blaze_tdf::TdfType) -> &'static str {
    match t {
        blaze_tdf::TdfType::Integer => "int",
        blaze_tdf::TdfType::String => "string",
        blaze_tdf::TdfType::Blob => "blob",
        blaze_tdf::TdfType::Struct => "struct",
        blaze_tdf::TdfType::List => "list",
        blaze_tdf::TdfType::Map => "map",
        blaze_tdf::TdfType::Union => "union",
        blaze_tdf::TdfType::Variable => "variable",
        blaze_tdf::TdfType::ObjectType => "object_type",
        blaze_tdf::TdfType::ObjectId => "object_id",
        blaze_tdf::TdfType::Float => "float",
        blaze_tdf::TdfType::TimeValue => "time",
        blaze_tdf::TdfType::Generic => "generic",
        blaze_tdf::TdfType::Unknown => "unknown",
    }
}
