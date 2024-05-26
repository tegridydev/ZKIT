use halo2_proofs::{
    arithmetic::{FieldExt, Field},
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{create_proof, keygen_pk, keygen_vk, verify_proof, Circuit, ConstraintSystem, Error, ProvingKey, VerifyingKey, Selector, Advice, Column, Rotation},
    poly::{commitment::{Params, ParamsProver}, EvaluationDomain, Polynomial},
    transcript::{ChallengeScalar, EncodedChallenge, Transcript},
};
use halo2_proofs::pasta::Fp;
use rand::rngs::OsRng;
use std::sync::Mutex;
use std::marker::PhantomData;
use std::collections::HashMap;
use std::io::{self, Write};

#[derive(Clone, Debug)]
struct CompressedData {
    data: Vec<Fp>,
}

impl CompressedData {
    fn new(data: Vec<Fp>) -> Self {
        Self { data }
    }
}

fn ingest_and_compress(data: Vec<u8>) -> CompressedData {
    let data_fp: Vec<Fp> = data.iter().map(|&x| Fp::from(x as u64)).collect();
    let poly = Polynomial::from_vec(data_fp.clone());
    let compressed_data = poly.to_vec();
    CompressedData::new(compressed_data)
}

struct ExampleCircuit<F: FieldExt> {
    pub data: Vec<F>,
    _marker: PhantomData<F>,
}

impl<F: FieldExt> Circuit<F> for ExampleCircuit<F> {
    type Config = ExampleConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self {
            data: vec![],
            _marker: PhantomData,
        }
    }

    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        let input = meta.advice_column();
        let s = meta.selector();

        meta.create_gate("data processing", |v_cells| {
            let input_exp = v_cells.query_advice(input, Rotation::cur());
            let s = v_cells.query_selector(s);

            vec![s * input_exp]
        });

        ExampleConfig {
            input,
            s,
        }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<F>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "data processing",
            |mut region| {
                config.s.enable(&mut region, 0)?;

                for (idx, &value) in self.data.iter().enumerate() {
                    region.assign_advice(|| "input", config.input, idx, || Value::known(value))?;
                }

                Ok(())
            },
        )?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct ExampleConfig {
    input: Column<Advice>,
    s: Selector,
}

struct ZKIT {
    storage: Mutex<HashMap<u64, CompressedData>>,
    zkio_counter: Mutex<u64>,
    params: Params<Fp>,
    vk: Option<VerifyingKey<Fp>>,
    pk: Option<ProvingKey<Fp>>,
}

impl ZKIT {
    fn new(params: Params<Fp>) -> Self {
        Self {
            storage: Mutex::new(HashMap::new()),
            zkio_counter: Mutex::new(0),
            params,
            vk: None,
            pk: None,
        }
    }

    fn setup_keys(&mut self, circuit: &impl Circuit<Fp>) -> Result<(), Error> {
        let vk = keygen_vk(&self.params, circuit)?;
        let pk = keygen_pk(&self.params, vk.clone(), circuit)?;
        self.vk = Some(vk);
        self.pk = Some(pk);
        Ok(())
    }

    fn batch_and_inscribe(&self, data: Vec<u8>) -> u64 {
        let compressed_data = ingest_and_compress(data);
        let mut storage = self.storage.lock().unwrap();
        let mut zkio_counter = self.zkio_counter.lock().unwrap();
        *zkio_counter += 1;
        storage.insert(*zkio_counter, compressed_data);
        *zkio_counter
    }

    fn create_proof(&self, circuit: &impl Circuit<Fp>) -> Result<Vec<u8>, Error> {
        let pk = self.pk.as_ref().expect("ProvingKey not set up");
        let mut transcript = Vec::new();
        create_proof(
            &self.params,
            pk,
            &[circuit],
            &[&[]],
            OsRng,
            &mut transcript,
        )?;
        Ok(transcript)
    }

    fn verify_proof(&self, proof: &[u8]) -> Result<bool, Error> {
        let vk = self.vk.as_ref().expect("VerifyingKey not set up");
        let mut transcript = proof.to_vec();
        verify_proof(&self.params, vk, &[&[]], &mut transcript)?;
        Ok(true)
    }

    fn retrieve_data(&self, index: u64) -> Option<Vec<u8>> {
        let storage = self.storage.lock().unwrap();
        storage.get(&index).map(|d| d.data.iter().map(|&fp| fp.get_lower_32() as u8).collect())
    }
}

fn main() {
    let params: Params<Fp> = Params::new(1 << 8);
    let mut zkit = ZKIT::new(params);

    // Setup keys with an example circuit
    let example_circuit = ExampleCircuit {
        data: vec![Fp::from(1), Fp::from(2), Fp::from(3)],
        _marker: PhantomData,
    };
    zkit.setup_keys(&example_circuit).unwrap();

    loop {
        println!("ZKIT Blockchain Simulation");
        println!("1. Ingest Data");
        println!("2. Create Proof");
        println!("3. Verify Proof");
        println!("4. Retrieve Data");
        println!("5. Exit");
        print!("Enter your choice: ");
        io::stdout().flush().unwrap();

        let mut choice = String::new();
        io::stdin().read_line(&mut choice).unwrap();
        let choice: u32 = choice.trim().parse().unwrap();

        match choice {
            1 => {
                let mut data = String::new();
                print!("Enter data to ingest (comma separated bytes): ");
                io::stdout().flush().unwrap();
                io::stdin().read_line(&mut data).unwrap();
                let data: Vec<u8> = data.trim().split(',').map(|x| x.trim().parse().unwrap()).collect();
                let id = zkit.batch_and_inscribe(data);
                println!("Data ingested with ID: {}", id);
            }
            2 => {
                let proof = zkit.create_proof(&example_circuit).unwrap();
                println!("Proof created successfully: {:?}", proof);
            }
            3 => {
                let mut proof = String::new();
                print!("Enter proof to verify (hex string): ");
                io::stdout().flush().unwrap();
                io::stdin().read_line(&mut proof).unwrap();
                let proof = hex::decode(proof.trim()).unwrap();
                if zkit.verify_proof(&proof).unwrap() {
                    println!("Proof verified successfully.");
                } else {
                    println!("Proof verification failed.");
                }
            }
            4 => {
                let mut index = String::new();
                print!("Enter data ID to retrieve: ");
                io::stdout().flush().unwrap();
                io::stdin().read_line(&mut index).unwrap();
                let index: u64 = index.trim().parse().unwrap();
                if let Some(data) = zkit.retrieve_data(index) {
                    println!("Retrieved data: {:?}", data);
                } else {
                    println!("Data not found.");
                }
            }
            5 => break,
            _ => println!("Invalid choice, please try again."),
        }
    }
}
