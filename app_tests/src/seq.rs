// Subject to the ISC licence (LICENSE-ISC.txt).

//! Implement Pippin support for sequences (vectors). This includes code for
//! classification by length, and is probably not something you'd want to copy
//! directly into a real application.

use std::io::Write;
use std::cell::Cell;
use std::u32;
use std::mem::size_of;
use std::fmt::Debug;

use rand::Rng;
use rand::distributions::{IndependentSample, Range, Normal, LogNormal};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use pippin::pip::*;


// —————  Sequence type itself  —————

/// Type of sequence elements.
pub type R = f64;
/// Type is a wrapper around a vector of f64. The reason for this is that we
/// can only implement `Element` for new types, thus cannot use the vector type
/// directly (see #44).
#[derive(PartialEq, Debug)]
pub struct Sequence {
    v: Vec<R>,
}
impl Eq for Sequence {}
impl Sequence {
    // Get length of sequence
    pub fn len(&self) -> usize {
        self.v.len()
    }
}
impl From<Vec<R>> for Sequence {
    fn from(v: Vec<R>) -> Self {
        Sequence { v: v }
    }
}

impl Element for Sequence {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        for x in &self.v {
            writer.write_f64::<LittleEndian>(*x)?;
        }
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        if buf.len() % size_of::<R>() != 0 {
            return OtherError::err("invalid data length");
        }
        let mut r: &mut &[u8] = &mut &buf[..];
        let n = buf.len() / size_of::<R>();
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(r.read_f64::<LittleEndian>()?);
        }
        Ok(Sequence{ v: v })
    }
}


// —————  Generators  —————
/// A generator can generate a sequence of numbers.
pub trait Generator: Debug {
    /// Generate a sequence of `n` numbers.
    fn generate(&self, n: usize) -> Vec<R>;
}
/// Arithmetic sequence (e.g. 1, 4, 7, 10)
#[derive(Debug)]
pub struct Arithmetic { start: R, step: R }
/// Geometric sequence (e.g. 2, 6, 18, 54)
#[derive(Debug)]
pub struct Geometric { start: R, factor: R }
/// Fibonacci sequence (usually 1, 1, 2, 3, 5, 8, ..., but starting numbers
/// can be changed)
#[derive(Debug)]
pub struct Fibonacci { x1: R, x2: R }
/// Power sequence (e.g. 3, 9, 27, 81)
#[derive(Debug)]
pub struct Power { e: R }

impl Generator for Arithmetic {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut x = self.start;
        while v.len() < n {
            v.push(x);
            x += self.step;
        }
        v
    }
}
impl Generator for Geometric {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut x = self.start;
        while v.len() < n {
            v.push(x);
            x *= self.factor;
        }
        v
    }
}
impl Generator for Fibonacci {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let (mut x1, mut x2) = (self.x1, self.x2);
        while v.len() < n {
            v.push(x1);
            let x = x1 + x2;
            x1 = x2;
            x2 = x;
        }
        v
    }
}
impl Generator for Power {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut i: R = 0.0;
        while v.len() < n {
            v.push(i.powf(self.e));
            i += 1.0;
        }
        v
    }
}

/// Enum of all generator types
#[derive(Debug)]
pub enum GeneratorEnum {
    Arithmetic(Arithmetic),
    Geometric(Geometric),
    Fibonacci(Fibonacci),
    Power(Power),
}
impl GeneratorEnum {
    /// Randomly create a new generator.
    pub fn new_random(mut rng: &mut Rng) -> GeneratorEnum {
        match Range::new(0, 4).ind_sample(&mut rng) {
            0 => {
                GeneratorEnum::Arithmetic(Arithmetic {
                    start: LogNormal::new(0., 100.).ind_sample(&mut rng),
                    step: Normal::new(0., 10.).ind_sample(&mut rng),
                })
            },
            1 => {
                GeneratorEnum::Geometric(Geometric {
                    start: LogNormal::new(0., 100.).ind_sample(&mut rng),
                    factor: Normal::new(0., 2.).ind_sample(&mut rng),
                })
            },
            2 => {
                GeneratorEnum::Fibonacci(Fibonacci {
                    x1: Normal::new(1., 1.).ind_sample(&mut rng),
                    x2: Normal::new(1., 1.).ind_sample(&mut rng),
                })
            },
            3 => {
                GeneratorEnum::Power(Power {
                    e: LogNormal::new(0., 1.).ind_sample(&mut rng),
                })
            },
            _ => { panic!("invalid sample"); }
        }
    }
}
impl Generator for GeneratorEnum {
    fn generate(&self, n: usize) -> Vec<R> {
        match self {
            &GeneratorEnum::Arithmetic(ref gen) => gen.generate(n),
            &GeneratorEnum::Geometric(ref gen) => gen.generate(n),
            &GeneratorEnum::Fibonacci(ref gen) => gen.generate(n),
            &GeneratorEnum::Power(ref gen) => gen.generate(n),
        }
    }
}


// —————  PartControl type  —————


/// Type implementing pippin's `PartControl`.
#[derive(Debug)]
pub struct SeqPartControl {
    time: Cell<i64>,
    io: Box<PartIO>,
    ss_policy: DefaultSnapshot,
}
impl SeqPartControl {
    /// Create, given I/O provider
    pub fn new(io: Box<PartIO>) -> Self {
        // time is start of year 2000
        SeqPartControl { time: Cell::new(946684800), io: io, ss_policy: Default::default() }
    }
}
// We can't use the default meta-data, with a real timestamp, as tests need
// to regenerate exactly the same data each time.
impl MakeCommitMeta for SeqPartControl {
    // add one hour
    fn make_commit_timestamp(&self) -> i64 {
        let time = self.time.get();
        self.time.set(time + 3600);
        time
    }
}
impl PartControl for SeqPartControl {
    type Element = Sequence;
    fn io(&self) -> &PartIO {
        &self.io
    }
    fn io_mut(&mut self) -> &mut PartIO {
        &mut self.io
    }
    fn snapshot_policy(&mut self) -> &mut SnapshotPolicy {
        &mut self.ss_policy
    }
    fn as_mcm_ref(&self) -> &MakeCommitMeta { self }
    fn as_mcm_ref_mut(&mut self) -> &mut MakeCommitMeta { self }
}



// —————  Properties  —————

/* TODO: properties?
/// Defined property functions
pub const PROP_SEQ_LEN: u32 = 1;

/// Property giving a sequence's length
fn prop_seq_len(elt: &Sequence) -> PropDomain {
    elt.v.len() as u32
}

impl<RIO: RepoIO> RepoControl for SeqControl<RIO> {
    type PartControl = SeqPartControl;
    type Element = Sequence;
    
    fn prop_fn(&self, id: PropId) -> Option<Property<Self::Element>> {
        match id {
            PROP_SEQ_LEN => Some(Property{ id, f: prop_seq_len }),
            _ => None
        }
    }
}
*/
