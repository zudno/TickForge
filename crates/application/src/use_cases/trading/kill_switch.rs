use futures::future::join_all;
use tickforge_domain::risk::KillSwitch;
use tickforge_domain::trading::OrderId;

use crate::error::ApplicationError;
use crate::ports::JournalEntry;
use crate::use_cases::trading::{CancelOrderUseCase, TradingPorts};

/// Resultado de activar el kill switch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KillSwitchReport {
    /// Órdenes cuya cancelación se solicitó con éxito.
    pub canceled: Vec<OrderId>,
    /// Órdenes que no se pudieron cancelar, con el motivo. Las órdenes nuevas siguen
    /// bloqueadas igualmente; el usuario puede reintentar.
    pub failed: Vec<(OrderId, String)>,
    /// El bloqueo quedó activo pero no se pudo registrar en el diario.
    pub journal_warning: Option<String>,
}

/// Caso de uso: interruptor de emergencia.
///
/// Activarlo **primero** guarda el bloqueo (desde ese instante la política de riesgo rechaza
/// toda orden nueva) y **después** cancela las órdenes abiertas. Así, aunque una cancelación
/// falle o la aplicación se caiga a mitad, nunca queda la puerta abierta a órdenes nuevas.
pub struct KillSwitchUseCase {
    ports: TradingPorts,
    cancel_order: CancelOrderUseCase,
}

impl KillSwitchUseCase {
    pub fn new(ports: TradingPorts) -> Self {
        Self {
            cancel_order: CancelOrderUseCase::new(ports.clone()),
            ports,
        }
    }

    pub async fn status(&self) -> Result<KillSwitch, ApplicationError> {
        Ok(self.ports.kill_switch.load().await?)
    }

    /// Activa el bloqueo y cancela todas las órdenes abiertas. Es seguro repetirlo: una
    /// segunda activación vuelve a intentar las cancelaciones pendientes.
    pub async fn engage(&self, reason: &str) -> Result<KillSwitchReport, ApplicationError> {
        let ports = &self.ports;
        let now = ports.clock.now();

        // 1. Bloquear: si esto falla, no se continúa (el usuario debe saber que no hay bloqueo)
        ports
            .kill_switch
            .save(&KillSwitch::engaged(now, reason))
            .await?;

        let mut report = KillSwitchReport::default();
        if let Err(error) = ports
            .journal
            .append(JournalEntry::KillSwitchEngaged {
                reason: reason.to_owned(),
                at: now,
            })
            .await
        {
            report.journal_warning = Some(error.to_string());
        }

        // 2. Cancelar todo lo cancelable, en paralelo
        let open_orders = ports.broker.open_orders().await?;
        let cancelable: Vec<OrderId> = open_orders
            .iter()
            .filter(|order| order.status().is_cancelable())
            .map(|order| order.id().clone())
            .collect();
        let results = join_all(
            cancelable
                .iter()
                .map(|order_id| self.cancel_order.execute(order_id)),
        )
        .await;

        for (order_id, result) in cancelable.into_iter().zip(results) {
            match result {
                Ok(()) => report.canceled.push(order_id),
                Err(error) => report.failed.push((order_id, error.to_string())),
            }
        }
        Ok(report)
    }

    /// Bloquea las órdenes nuevas **sin cancelar** las abiertas. Se usa al iniciar una
    /// sesión en modo real: la persona revisa la cuenta y desbloquea a propósito. Si ya
    /// estaba activo, conserva el motivo y la hora originales.
    pub async fn hold(&self, reason: &str) -> Result<KillSwitch, ApplicationError> {
        let ports = &self.ports;
        let current = ports.kill_switch.load().await?;
        if current.is_engaged() {
            return Ok(current);
        }

        let now = ports.clock.now();
        let engaged = KillSwitch::engaged(now, reason);
        ports.kill_switch.save(&engaged).await?;
        // A diferencia de `engage`, aquí no hay nada urgente que proteger: si el diario
        // falla se informa el error (el bloqueo ya quedó guardado)
        ports
            .journal
            .append(JournalEntry::KillSwitchEngaged {
                reason: reason.to_owned(),
                at: now,
            })
            .await?;
        Ok(engaged)
    }

    /// Desactiva el bloqueo. No reabre nada: solo vuelve a permitir órdenes nuevas.
    pub async fn release(&self) -> Result<(), ApplicationError> {
        let ports = &self.ports;
        ports.kill_switch.save(&KillSwitch::Released).await?;
        // El diario es auditoría: si falla, el desbloqueo ya está hecho y se informa como error
        ports
            .journal
            .append(JournalEntry::KillSwitchReleased {
                at: ports.clock.now(),
            })
            .await?;
        Ok(())
    }
}
