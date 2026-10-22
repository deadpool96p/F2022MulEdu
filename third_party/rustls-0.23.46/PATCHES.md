# Patches applied to the vendored `rustls` 0.23.46

This directory is an unmodified copy of the crates.io release `rustls-0.23.46`
(registry path:
`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rustls-0.23.46`),
with **two** files patched. Everything else is byte-identical to the upstream
release; the inner `Cargo.lock` was removed (it is not part of the published
crate and only confuses `cargo` when the copy is used as a path dependency).

The copy is wired in from the workspace root:

```toml
[patch.crates-io]
rustls = { path = "third_party/rustls-0.23.46" }
```

and the workspace `exclude` list keeps cargo from trying to treat it as a
workspace member.

## Why the patch is needed

EA's client dials the emulator by **bare IP address** in some flows. A URL
whose host is an IP literal makes the client emit a TLS `server_name`
extension whose `host_name` entry has a **zero-length** hostname. Upstream
rustls reads that hostname with `PayloadU16::<NonEmpty>::read`, which fails
with `InvalidMessage::IllegalEmptyValue` and kills the handshake with a fatal
alert — the client then aborts the whole login sequence.

This matters for this project specifically: the emulator is hosted on a Radmin
VPN address (26.178.140.32), and remote friends connect to that address
directly, so **every** TLS connection they make carries an empty SNI.

## Why the patch is a *parse-level* change only

A TLS 1.3 handshake transcript is computed over the **raw ClientHello bytes**
the client sent (`rustls::hash_hs::MessagePayload::Handshake` calls
`add_raw(encoded.bytes())`). Any byte-level rewrite of the ClientHello — for
example stripping the `server_name` extension before handing the socket to
rustls — invalidates the client's `Finished` and produces a `DecryptError`.

So the client's bytes must be parsed exactly as they arrived. The only safe
place to intervene is how the parsed value is *interpreted*, which is what this
patch does.

## The diff

### 1. `src/msgs/handshake.rs`

`ServerNamePayload` gains an `Empty` variant, and `HostNamePayload::read`
reads the hostname permissively so that a zero-length one becomes
`HostNamePayload::Empty` instead of an `InvalidMessage` error.

```diff
@@ -273,6 +273,14 @@
 
     /// A successfully decoded, but syntactically-invalid value.
     Invalid,
+
+    /// The client sent a `server_name` extension whose hostname is empty.
+    ///
+    /// RFC 6066 has no way to say "no name at all", so a client that
+    /// connects to a bare IP address sends a `host_name` entry with a
+    /// zero-length hostname. The name carries no information, so it is
+    /// treated exactly like no `server_name` extension.
+    Empty,
 }
 
 impl ServerNamePayload<'_> {
@@ -281,6 +289,7 @@
             Self::SingleDnsName(d) => ServerNamePayload::SingleDnsName(d.to_owned()),
             Self::IpAddress => ServerNamePayload::IpAddress,
             Self::Invalid => ServerNamePayload::Invalid,
+            Self::Empty => ServerNamePayload::Empty,
         }
     }
 
@@ -358,6 +367,11 @@
                     );
                     Some(Self::Invalid)
                 }
+
+                HostNamePayload::Empty => {
+                    warn!("Empty SNI hostname received: treating it as no server name");
+                    Some(Self::Empty)
+                }
             };
         }
 
@@ -376,12 +390,25 @@
     HostName(DnsName<'static>),
     IpAddress(PayloadU16<NonEmpty>),
     Invalid(PayloadU16<NonEmpty>),
+    /// A zero-length hostname.
+    ///
+    /// A client that connects to a bare IP address has no DNS name to send,
+    /// but still emits a `host_name` entry with an empty hostname.
+    Empty,
 }
 
 impl HostNamePayload {
     fn read(r: &mut Reader<'_>) -> Result<Self, InvalidMessage> {
         use pki_types::ServerName;
-        let raw = PayloadU16::<NonEmpty>::read(r)?;
+
+        // An empty hostname is read as `MaybeEmpty` so that it can be
+        // reported as "no server name" rather than failing the whole
+        // ClientHello with `IllegalEmptyValue`.
+        let raw = PayloadU16::<MaybeEmpty>::read(r)?;
+        if raw.0.is_empty() {
+            return Ok(Self::Empty);
+        }
+        let raw = PayloadU16::<NonEmpty>::new(raw.0);
 
         match ServerName::try_from(raw.0.as_slice()) {
             Ok(ServerName::DnsName(d)) => Ok(Self::HostName(d.to_owned())),
```

### 2. `src/server/hs.rs`

`process_client_hello` maps the new variant to "no server name", the same
treatment an `IpAddress` SNI already receives. Note the difference from
`Invalid`: an `Invalid` SNI still produces a fatal `IllegalParameter` alert,
because an invalid (as opposed to absent) name is a protocol error worth
rejecting. An *empty* name is not an error — it is what a client sends when it
has no name to give.

```diff
@@ -747,6 +747,7 @@
     let sni = match &client_hello.server_name {
         Some(ServerNamePayload::SingleDnsName(dns_name)) => Some(dns_name.to_lowercase_owned()),
         Some(ServerNamePayload::IpAddress) => None,
+        Some(ServerNamePayload::Empty) => None,
         Some(ServerNamePayload::Invalid) => {
             return Err(cx.common.send_fatal_alert(
                 AlertDescription::IllegalParameter,
```

## Scope of the change

* **No cryptographic or transcript change.** The ClientHello bytes are neither
  rewritten nor re-encoded; the handshake transcript and the client's
  `Finished` are computed over exactly the same bytes as before.
* **No change for normal clients.** A ClientHello with a real DNS name takes
  the identical `HostName` path as upstream. A ClientHello with *no*
  `server_name` extension at all is unaffected (the extension is not parsed).
* **Certificate selection is unaffected.** The server picks its certificate
  from its own config, not from the SNI, so an empty SNI still gets the
  emulator's certificate — the same one that carries the `*.ea.com` SANs.
* **Verification.** `cargo test --workspace` is green (173 passed, 0 failed)
  with this patch applied.

## Reverting

Remove the `[patch.crates-io]` block from the workspace root `Cargo.toml` and
delete `third_party/`; cargo will then resolve `rustls` from crates.io again.
