/// A vertex (graph node) that carries the attributes required to compute
/// modal metrics such as the Walking Comfort Index (WCI) or Level of Traffic Stress (LTS).
///
/// Implement this trait for a map provider's vertex/node type
/// (e.g. OpenStreetMap node, Overture Maps connector).
pub trait VertexForModalMetric {
    // WCI - only
    /// returns true if the vertex has a traffic signal.
    fn has_traffic_signals(&self) -> bool;
    /// returns true if the vertex has a stop sign.
    fn has_stop_sign(&self) -> bool;
}

/// Compact 1-byte representation of [`VertexForModalMetric`] attributes so that
/// full vertex records (strings, coordinates, infrastructure ID lists) do not need
/// to remain in memory during bulk modal metric computation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompactVertexAttrs(u8);

impl CompactVertexAttrs {
    const FLAG_TRAFFIC_SIGNALS: u8 = 1 << 0;
    const FLAG_STOP_SIGN: u8 = 1 << 1;

    #[inline]
    pub fn from_vertex<V: VertexForModalMetric>(vertex: &V) -> Self {
        let mut bits = 0u8;
        if vertex.has_traffic_signals() {
            bits |= Self::FLAG_TRAFFIC_SIGNALS;
        }
        if vertex.has_stop_sign() {
            bits |= Self::FLAG_STOP_SIGN;
        }
        Self(bits)
    }
}

impl VertexForModalMetric for CompactVertexAttrs {
    #[inline]
    fn has_traffic_signals(&self) -> bool {
        self.0 & Self::FLAG_TRAFFIC_SIGNALS != 0
    }

    #[inline]
    fn has_stop_sign(&self) -> bool {
        self.0 & Self::FLAG_STOP_SIGN != 0
    }
}
