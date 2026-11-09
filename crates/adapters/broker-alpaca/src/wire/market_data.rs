use serde::Deserialize;
use serde_json::Number;

/// `GET /v2/stocks/{symbol}/quotes/latest`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LatestQuoteResponseWire {
    pub quote: QuoteWire,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QuoteWire {
    /// Marca de tiempo RFC 3339.
    pub t: String,
    /// Precio bid; `0` cuando no hay ofertas de compra.
    pub bp: Number,
    pub bs: Number,
    /// Precio ask; `0` cuando no hay ofertas de venta.
    pub ap: Number,
    #[serde(rename = "as")]
    pub ask_size: Number,
}

/// `GET /v2/stocks/{symbol}/bars`
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BarsResponseWire {
    /// Puede llegar como `null` cuando no hay velas en el rango.
    pub bars: Option<Vec<BarWire>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BarWire {
    pub t: String,
    pub o: Number,
    pub h: Number,
    pub l: Number,
    pub c: Number,
    pub v: Number,
}
