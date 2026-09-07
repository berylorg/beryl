# Reason For Investigation

Audit SP-007 evaluated whether url could replace the custom provider image-locator validator,
which processes arbitrary-length text across fragments with constant retained state.

# Outcome

Retain the current state machine and its existing std::net::Ipv6Addr reuse. url 2.5.8 accepts a
complete &str and owns a serialization String; ParseOptions::parse allocates capacity based on
the input length. Its WHATWG parsing and normalization also differ from Beryl's exact unnormalized
locator grammar. An allocated URL object does not remove streamed validation responsibilities.

Beryl applies the same grammar to materialized, streamed-frame and staged-observation text, rejects
data-image locators with a distinct error, and uses only a fixed small IPv6 buffer. A replacement
would need an explicit residency and accepted-syntax decision plus exact-byte/error adapters.
No compatible reduction or adoption recommendation is established.

Registry metadata checked on 2026-09-06 confirms version 2.5.8, dual MIT/Apache-2.0 licensing,
Rust 1.63 and default std. That feature enables std support in idna, percent-encoding and
form_urlencoded. Allocation and grammar mismatch ended this assessment before broad adoption,
target, benchmark or advisory qualification. No package was installed or changed and no tests ran.

# Sources

- [Exact registry metadata](https://crates.io/api/v1/crates/url/2.5.8).
- [Versioned API](https://docs.rs/url/2.5.8/url/).
- [Official source](https://docs.rs/url/latest/src/url/lib.rs.html), reviewed at 2.5.8:
  Url fields at lines 228-250 and ParseOptions::parse at lines 305-315.
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: provider_item/structured/locator.rs,
  locator/authority.rs, provider stream text, observation value validation and related tests.
  See [SP-007](../../../../audits/code-simplification/reviews/syndic-provider-findings.json).
