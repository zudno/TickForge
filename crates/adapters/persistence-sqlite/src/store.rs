use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::StoreError;

/// Esquema de una base: sus migraciones y la marca que identifica el archivo.
///
/// Migraciones en orden; la posición + 1 es la versión del esquema (`PRAGMA user_version`).
/// Se embeben al compilar: el binario no depende de archivos externos. La marca
/// (`PRAGMA application_id`) impide abrir una base con el esquema de la otra.
struct Schema {
    name: &'static str,
    application_id: i64,
    migrations: &'static [&'static str],
}

/// Base de trading (una por modo): diario de órdenes, watchlist y kill switch. Su marca es 0,
/// la que ya tenían las bases creadas antes de existir la de mercado.
const TRADING: Schema = Schema {
    name: "trading",
    application_id: 0,
    migrations: &[
        include_str!("../migrations/0001_initial.sql"),
        include_str!("../migrations/0002_kill_switch.sql"),
    ],
};

/// Base de mercado (`market.db`): histórico de velas para entrenar modelos.
const MARKET_DATA: Schema = Schema {
    name: "mercado",
    // "TFMD" en ASCII
    application_id: 0x5446_4d44,
    migrations: &[include_str!("../migrations/market/0001_bars.sql")],
};

/// Error al ejecutar una operación sobre la base ya abierta.
#[derive(Debug)]
pub(crate) enum DbError {
    Sqlite(rusqlite::Error),
    /// La tarea de fondo que ejecutaba la consulta falló o fue cancelada.
    Task(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "{error}"),
            Self::Task(reason) => write!(f, "tarea de base de datos interrumpida: {reason}"),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

/// Conexión compartida a la base SQLite, con el esquema ya migrado.
///
/// SQLite es una biblioteca síncrona: cada operación se ejecuta en un hilo de bloqueo de
/// tokio (`spawn_blocking`) para no detener la TUI ni las conexiones de red.
/// Es barata de clonar: todos los repositorios comparten la misma conexión.
#[derive(Debug, Clone)]
pub struct SqliteStore {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteStore {
    /// Abre (o crea) la base de trading en `path` y aplica las migraciones pendientes.
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        Self::open_schema(path, &TRADING).await
    }

    /// Abre (o crea) la base de mercado (histórico de velas) en `path`.
    pub async fn open_market_data(path: &Path) -> Result<Self, StoreError> {
        Self::open_schema(path, &MARKET_DATA).await
    }

    async fn open_schema(path: &Path, schema: &'static Schema) -> Result<Self, StoreError> {
        let path: PathBuf = path.to_path_buf();
        run_blocking(move || {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                std::fs::create_dir_all(parent).map_err(|error| StoreError::DataDirectory {
                    path: parent.display().to_string(),
                    reason: error.to_string(),
                })?;
            }
            let connection =
                Connection::open(&path).map_err(|error| StoreError::Open(error.to_string()))?;
            configure(&connection)?;
            Self::migrated(connection, schema)
        })
        .await?
    }

    /// Base de trading en memoria (se pierde al cerrar). Para pruebas.
    pub async fn in_memory() -> Result<Self, StoreError> {
        Self::in_memory_schema(&TRADING).await
    }

    /// Base de mercado en memoria. Para pruebas.
    pub async fn in_memory_market_data() -> Result<Self, StoreError> {
        Self::in_memory_schema(&MARKET_DATA).await
    }

    async fn in_memory_schema(schema: &'static Schema) -> Result<Self, StoreError> {
        run_blocking(move || {
            let connection = Connection::open_in_memory()
                .map_err(|error| StoreError::Open(error.to_string()))?;
            Self::migrated(connection, schema)
        })
        .await?
    }

    fn migrated(mut connection: Connection, schema: &Schema) -> Result<Self, StoreError> {
        migrate(&mut connection, schema)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Ejecuta `operation` con la conexión, fuera del hilo asíncrono.
    pub(crate) async fn with_connection<T, F>(&self, operation: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut Connection) -> Result<T, rusqlite::Error> + Send + 'static,
        T: Send + 'static,
    {
        let connection = Arc::clone(&self.connection);
        tokio::task::spawn_blocking(move || {
            // Si otro hilo entró en pánico con el candado, la conexión sigue siendo usable
            let mut connection = connection.lock().unwrap_or_else(PoisonError::into_inner);
            operation(&mut connection).map_err(DbError::from)
        })
        .await
        .map_err(|error| DbError::Task(error.to_string()))?
    }

    /// Vuelca el registro WAL al archivo principal. Conviene llamarlo al salir; si la
    /// aplicación se cierra sin hacerlo, SQLite lo recupera al abrir la próxima vez, así que
    /// un fallo aquí no pierde datos.
    pub async fn close(&self) {
        let _ = self
            .with_connection(|connection| {
                connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            })
            .await;
    }
}

/// Ajustes de la conexión para un archivo en disco.
fn configure(connection: &Connection) -> Result<(), StoreError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .and_then(|()| {
            // WAL: lecturas y escrituras sin bloquearse y tolerante a cortes de energía
            connection.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA foreign_keys = ON;",
            )
        })
        .map_err(|error| StoreError::Open(error.to_string()))
}

/// Aplica, en una transacción cada una, las migraciones posteriores a la versión actual.
fn migrate(connection: &mut Connection, schema: &Schema) -> Result<(), StoreError> {
    let migration_error = |error: rusqlite::Error| StoreError::Migration(error.to_string());
    let migrations = schema.migrations;

    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(migration_error)?;
    let application_id: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(migration_error)?;
    // Una base nueva (versión 0) recibe la marca de su esquema; una existente debe tenerla
    if version == 0 {
        connection
            .execute_batch(&format!(
                "PRAGMA application_id = {};",
                schema.application_id
            ))
            .map_err(migration_error)?;
    } else if application_id != schema.application_id {
        return Err(StoreError::Migration(format!(
            "el archivo no es una base de {} de TickForge (marca {application_id:#x})",
            schema.name
        )));
    }
    let current = usize::try_from(version)
        .map_err(|_| StoreError::Migration(format!("versión de esquema inválida: {version}")))?;
    if current > migrations.len() {
        return Err(StoreError::Migration(format!(
            "la base tiene la versión de esquema {current}, más nueva que esta aplicación ({})",
            migrations.len()
        )));
    }

    for (index, sql) in migrations.iter().enumerate().skip(current) {
        let transaction = connection.transaction().map_err(migration_error)?;
        transaction.execute_batch(sql).map_err(migration_error)?;
        transaction
            .execute_batch(&format!("PRAGMA user_version = {};", index + 1))
            .map_err(migration_error)?;
        transaction.commit().map_err(migration_error)?;
    }
    Ok(())
}

/// Ejecuta trabajo bloqueante de apertura en un hilo aparte.
async fn run_blocking<T, F>(work: F) -> Result<T, StoreError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| StoreError::Open(format!("tarea de apertura interrumpida: {error}")))
}
