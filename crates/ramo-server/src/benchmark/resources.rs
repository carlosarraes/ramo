/// Resource sampling is optional because Ollama may run outside this process.
///
/// Returning no value is preferable to reporting Ramo's own RSS as if it were
/// the model runner's memory use. A platform sampler can be added without
/// changing the public measurement format.
pub(crate) fn peak_rss_bytes() -> Option<u64> {
    None
}
