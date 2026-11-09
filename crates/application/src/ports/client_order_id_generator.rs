use tickforge_domain::DomainError;
use tickforge_domain::trading::ClientOrderId;

/// Genera `ClientOrderId` únicos para cada orden nueva.
///
/// La unicidad es lo que permite reintentar un envío sin duplicar la orden en el broker.
/// Es falible para que ninguna implementación necesite `unwrap` al validar el formato.
pub trait ClientOrderIdGenerator: Send + Sync {
    fn next_id(&self) -> Result<ClientOrderId, DomainError>;
}
