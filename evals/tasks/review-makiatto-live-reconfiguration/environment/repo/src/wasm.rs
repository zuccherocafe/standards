//! Compiled byte filters with invocation-local environment.

use wasmtime::{Engine, Linker, Module, Store};

use crate::config::TransformConfig;

#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    #[error("cannot read transform: {0}")]
    Read(#[from] std::io::Error),
    #[error("invalid WASM transform: {0}")]
    Wasm(#[from] wasmtime::Error),
    #[error("OFFSET must be an integer byte")]
    Offset,
}

pub struct Transforms {
    engine: Engine,
}

impl Default for Transforms {
    fn default() -> Self {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        Self {
            engine: Engine::new(&config).expect("supported WASM engine configuration"),
        }
    }
}

impl Transforms {
    pub fn apply(
        &mut self,
        config: &TransformConfig,
        input: &[u8],
    ) -> Result<Vec<u8>, TransformError> {
        let wasm_bytes = std::fs::read(&config.wasm_path)?;
        let component = Module::new(&self.engine, &wasm_bytes)?;
        execute(&self.engine, &component, config, input)
    }
}

fn execute(
    engine: &Engine,
    component: &Module,
    config: &TransformConfig,
    input: &[u8],
) -> Result<Vec<u8>, TransformError> {
    let offset: u8 = config
        .env
        .get("OFFSET")
        .map_or(Ok(0), |value| value.parse())
        .map_err(|_| TransformError::Offset)?;
    let mut store = Store::new(engine, offset);
    store.set_fuel(1_000_000)?;
    let mut linker = Linker::new(engine);
    linker.func_wrap("host", "offset", |caller: wasmtime::Caller<'_, u8>| {
        i32::from(*caller.data())
    })?;
    let instance = linker.instantiate(&mut store, component)?;
    let transform = instance.get_typed_func::<i32, i32>(&mut store, "transform_byte")?;
    input
        .iter()
        .map(|byte| {
            let result = transform.call(&mut store, i32::from(*byte))?;
            Ok(result.to_le_bytes()[0])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn file_transform_uses_environment() {
        let path = std::env::temp_dir().join(format!("makiatto-shift-{}.wat", std::process::id()));
        std::fs::write(
            &path,
            r#"(module
            (import "host" "offset" (func $offset (result i32)))
            (func (export "transform_byte") (param i32) (result i32)
                local.get 0 call $offset i32.add))"#,
        )
        .unwrap();
        let config = TransformConfig {
            wasm_path: path.clone(),
            env: BTreeMap::from([("OFFSET".into(), "1".into())]),
        };
        assert_eq!(Transforms::default().apply(&config, b"a").unwrap(), b"b");
        std::fs::remove_file(path).unwrap();
    }
}
