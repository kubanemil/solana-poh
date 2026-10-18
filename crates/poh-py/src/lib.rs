use std::sync::{Mutex, MutexGuard, PoisonError};

use poh_core::{Hash, PohEntry, VerifyError};
use pyo3::{exceptions::PyValueError, prelude::*, sync::MutexExt, types::PyBytes};

fn to_hash(bytes: &[u8]) -> PyResult<Hash> {
    Hash::try_from(bytes).map_err(|err| PyValueError::new_err(err.to_string()))
}

fn to_bytes<'py>(py: Python<'py>, hash: &Hash) -> Bound<'py, PyBytes> {
    PyBytes::new(py, hash.as_ref())
}

fn verify_error(py: Python<'_>, err: VerifyError) -> PyErr {
    let raised = py
        .import("solana_poh._errors")
        .and_then(|module| module.getattr("VerifyError"))
        .and_then(|class| class.call1((err.index, err.kind.to_string())));
    match raised {
        Ok(exception) => PyErr::from_value(exception),
        Err(import_failed) => import_failed,
    }
}

#[pyclass(module = "solana_poh", name = "PohEntry", frozen, eq, hash)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PyPohEntry(PohEntry);

#[pymethods]
impl PyPohEntry {
    #[new]
    #[pyo3(signature = (num_hashes, hash, mixin = None))]
    fn new(num_hashes: u64, hash: &[u8], mixin: Option<&[u8]>) -> PyResult<Self> {
        Ok(Self(PohEntry {
            num_hashes,
            hash: to_hash(hash)?,
            mixin: mixin.map(to_hash).transpose()?,
        }))
    }

    #[getter]
    fn num_hashes(&self) -> u64 {
        self.0.num_hashes
    }

    #[getter]
    fn hash<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        to_bytes(py, &self.0.hash)
    }

    #[getter]
    fn mixin<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.0.mixin.as_ref().map(|mixin| to_bytes(py, mixin))
    }

    #[getter]
    fn is_tick(&self) -> bool {
        self.0.is_tick()
    }

    fn __repr__(&self) -> String {
        let mixin = self.0.mixin.map_or("None".into(), |m| format!("'{m}'"));
        format!(
            "PohEntry(num_hashes={}, hash='{}', mixin={mixin})",
            self.0.num_hashes, self.0.hash
        )
    }
}

// Frozen with a Mutex, not PyO3's borrow flag: `hash` runs without the GIL, and
// another thread calling `record` or `tick` meanwhile should wait, not raise.
#[pyclass(module = "solana_poh", name = "Poh", frozen)]
#[derive(Debug)]
struct PyPoh(Mutex<poh_core::Poh>);

impl PyPoh {
    fn lock(&self, py: Python<'_>) -> MutexGuard<'_, poh_core::Poh> {
        self.0
            .lock_py_attached(py)
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[pymethods]
impl PyPoh {
    #[new]
    #[pyo3(signature = (start_hash, hashes_per_tick = None))]
    fn new(start_hash: &[u8], hashes_per_tick: Option<u64>) -> PyResult<Self> {
        if matches!(hashes_per_tick, Some(0 | 1)) {
            return Err(PyValueError::new_err(
                "hashes_per_tick must be greater than 1",
            ));
        }
        let poh = poh_core::Poh::new(to_hash(start_hash)?, hashes_per_tick);
        Ok(Self(Mutex::new(poh)))
    }

    fn hash(&self, py: Python<'_>, max_num_hashes: u64) -> bool {
        py.detach(|| {
            let mut poh = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            poh.hash(max_num_hashes)
        })
    }

    fn record(&self, py: Python<'_>, mixin: &[u8]) -> PyResult<Option<PyPohEntry>> {
        let mixin = to_hash(mixin)?;
        Ok(self.lock(py).record(mixin).map(PyPohEntry))
    }

    fn tick(&self, py: Python<'_>) -> Option<PyPohEntry> {
        self.lock(py).tick().map(PyPohEntry)
    }

    fn remaining_hashes_in_slot(&self, py: Python<'_>, ticks_per_slot: u64) -> PyResult<u64> {
        if ticks_per_slot == 0 {
            return Err(PyValueError::new_err("ticks_per_slot must be positive"));
        }
        Ok(self.lock(py).remaining_hashes_in_slot(ticks_per_slot))
    }

    #[getter]
    fn current_hash<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        let hash = self.lock(py).current_hash();
        to_bytes(py, &hash)
    }

    #[getter]
    fn num_hashes(&self, py: Python<'_>) -> u64 {
        self.lock(py).num_hashes()
    }

    #[getter]
    fn hashes_per_tick(&self, py: Python<'_>) -> Option<u64> {
        self.lock(py).hashes_per_tick()
    }

    #[getter]
    fn remaining_hashes_until_tick(&self, py: Python<'_>) -> u64 {
        self.lock(py).remaining_hashes_until_tick()
    }

    #[getter]
    fn tick_number(&self, py: Python<'_>) -> u64 {
        self.lock(py).tick_number()
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let poh = self.lock(py);
        format!(
            "Poh(current_hash='{}', num_hashes={}, remaining_hashes_until_tick={})",
            poh.current_hash(),
            poh.num_hashes(),
            poh.remaining_hashes_until_tick()
        )
    }
}

fn collect(entries: &[Bound<'_, PyPohEntry>]) -> Vec<PohEntry> {
    entries.iter().map(|entry| entry.get().0).collect()
}

#[pyfunction]
fn verify(py: Python<'_>, start_hash: &[u8], entries: Vec<Bound<'_, PyPohEntry>>) -> PyResult<()> {
    let (start, entries) = (to_hash(start_hash)?, collect(&entries));
    py.detach(|| poh_core::verify(&start, &entries))
        .map_err(|err| verify_error(py, err))
}

#[pyfunction]
fn verify_parallel(
    py: Python<'_>,
    start_hash: &[u8],
    entries: Vec<Bound<'_, PyPohEntry>>,
) -> PyResult<()> {
    let (start, entries) = (to_hash(start_hash)?, collect(&entries));
    py.detach(|| poh_core::verify_parallel(&start, &entries))
        .map_err(|err| verify_error(py, err))
}

#[pyfunction]
#[pyo3(signature = (entries, hashes_per_tick, tick_hash_count = 0))]
fn verify_tick_hash_count(
    py: Python<'_>,
    entries: Vec<Bound<'_, PyPohEntry>>,
    hashes_per_tick: u64,
    mut tick_hash_count: u64,
) -> PyResult<u64> {
    poh_core::verify_tick_hash_count(&collect(&entries), &mut tick_hash_count, hashes_per_tick)
        .map_err(|err| verify_error(py, err))?;
    Ok(tick_hash_count)
}

#[pyfunction]
fn hash_signatures<'py>(
    py: Python<'py>,
    signatures: Vec<Bound<'py, PyBytes>>,
) -> Bound<'py, PyBytes> {
    let signatures: Vec<&[u8]> = signatures.iter().map(|s| s.as_bytes()).collect();
    let root = py.detach(|| poh_core::hash_signatures(&signatures));
    to_bytes(py, &root)
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPoh>()?;
    m.add_class::<PyPohEntry>()?;
    m.add_function(wrap_pyfunction!(verify, m)?)?;
    m.add_function(wrap_pyfunction!(verify_parallel, m)?)?;
    m.add_function(wrap_pyfunction!(verify_tick_hash_count, m)?)?;
    m.add_function(wrap_pyfunction!(hash_signatures, m)?)?;
    Ok(())
}
