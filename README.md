# rust-hll

A Rust implementation of [HyperLogLog](http://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf) that is
storage-compatible with the [Aggregate Knowledge HLL Storage Spec](https://github.com/aggregateknowledge/hll-storage-spec).

## Overview
HyperLogLog (HLL) is a fixed-size, set-like structure used for distinct value counting with tunable precision. For
example, in 1280 bytes HLL can estimate the count of tens of billions of distinct values with only a few percent error.

In addition to the algorithm proposed in the [original paper](http://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf),
this implementation is augmented to improve its accuracy and memory use without sacrificing much speed.

## Motivation
While there are a handful of existing HLL implementations in Rust, none of them (that I have found) implement the AK Storage Spec.   
The unified storage format is useful for reading and writing HLLs in a multi-lingual environment. This implementation
allows for seamless integration with other HLL implementations that follow the AK Storage Spec, such as the PostgreSQL HLL
extension.

## Hashing
A good hashing algorithm is crucial to achieving the pseudorandomness that HLL requires in order to perform its 
calculations. The 64-bit variant of [MurmurHash3](https://github.com/spaolacci/murmur3) is recommended. If using a 
seed, it must be constant for all inputs to a given HLL. Further, if that HLL is to be unioned, then the same seed must
be used for all inputs to the other HLL.

## Usage

```rust
use rust_hll::{Hll, Settings};

fn main() {
    // Create settings for the HLL
    let settings = Settings::new(
        10,   // log_2m: number of registers will be 2^10
        4,    // reg_width: 4 bits per register
        -1,   // explicit_threshold: auto-calculate threshold
        true, // sparse_enabled: use sparse representation
    )
    .unwrap();

    // Create a new HLL with the settings
    let mut hll = Hll::new(settings);

    // Add elements
    hll.add_raw(123456789);
    println!("Cardinality: {}", hll.cardinality()); // prints "1"

    // Create another HLL and add elements
    let mut hll2 = Hll::new(settings);
    hll2.add_raw(123456789);
    hll2.add_raw(987654321);

    // Union HLLs
    hll2.union(true, &hll).unwrap();
    println!("Cardinality after union: {}", hll2.cardinality()); // prints "2"

    // Serialize to bytes
    let bytes = hll2.to_bytes();

    // Deserialize from bytes
    let hll3 = Hll::from_bytes(&bytes).unwrap();
    println!("Cardinality after deserialization: {}", hll3.cardinality()); // prints "2"
}
```

## Configuration Options

The HLL implementation can be configured through the `Settings` struct:

- `log_2m`: Determines the number of registers in the HLL (2^log_2m). Must be between 4 and 31.
- `reg_width`: Number of bits dedicated to each register value. Must be between 1 and 8.
- `explicit_threshold`: Cardinality at which the HLL transitions from explicit to probabilistic storage. Use -1 for auto-calculation.
- `sparse_enabled`: Whether to use sparse representation. When true, conversion thresholds are automatically calculated.

## Storage Types

The implementation uses three storage types that automatically transition based on the data:

1. **Explicit**: Used for small cardinalities, stores actual values
2. **Sparse**: Used for medium cardinalities, stores only non-zero registers
3. **Dense**: Used for large cardinalities, stores all registers

## Additional Resources
* [HyperLogLog: the analysis of a near-optimal cardinality estimation algorithm](http://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf)
* [Understanding the HyperLogLog](https://pdfs.semanticscholar.org/75ba/51ffd9d2bed8a65029c9340d058f587059da.pdf)

## License
Released under the [MIT license](LICENSE). 