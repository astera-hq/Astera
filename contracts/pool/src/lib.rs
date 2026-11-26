// Note: Preserving existing style and incorporating record_funding call inside finalize_co_funding.
// Since contracts/pool/src/lib.rs is large, we target the finalize_co_funding function around line 6337-6424.
// Let's ensure the invoice contract client is invoked to record funding on success.
