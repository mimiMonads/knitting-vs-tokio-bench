# Benchmark Summary

## Sources

- tokio: `results/tokio-1780359436688.csv`
- bun: `results/knitting-bun-1780359504852.csv`
- node: `results/knitting-node-1780359653274.csv`
- deno: `results/knitting-deno-1780359560428.csv`

## Machine Specs

- OS: Ubuntu 24.04.4 LTS
- Kernel: 6.17.0-29-generic
- Architecture: x86_64
- CPU: 11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz
- Topology: 8 logical CPUs, 1 socket(s), 4 core(s)/socket, 2 thread(s)/core
- Memory: 15.4 GiB
- Swap: 4.0 GiB

## Methodology Notes

- The main string and byte benchmarks are intended to compare the same logical round trip on both sides: send payload, receive it in the worker, echo it back, receive it again on the caller, then wait for the whole batch.
- In `src/main.ts`, the `string` and `Uint8Array` paths go through knitting transport in both directions. That transport materializes a fresh payload on receive, so the round trip includes payload work on both the request side and the reply side.
- To keep the Tokio baseline fair, `src/main.rs` clones `String` and `Vec<u8>` on send and also clones again on the worker reply. The reply clone is intentional. Without it, Tokio would be measuring a cheaper return-path move while the JS runtimes were still paying for fresh payload materialization on the way back.
- The `Arc<Vec<u8>>` sweep is intentionally separate and is not the default apples-to-apples byte benchmark. It exists as an upper-bound shared-bytes reference for small payloads. `Arc::clone` only bumps a refcount, so it is expected to be cheaper than copying bytes.
- This means the default `string` and `Uint8Array` tables should be read as the fairer comparison, while the Arc section should be read as "how close does the normal transport get to shared ownership for small values?"

## Batch Avg Latency (less is better)

```text
benchmark            | batch | tokio     | bun       | node     | deno     
---------------------+-------+-----------+-----------+----------+----------
number f64 (8 bytes) | n=1   | 4.57 us   | -         | -        | -        
number f64 (8 bytes) | n=10  | 9.60 us   | -         | -        | -        
number f64 (8 bytes) | n=100 | 52.84 us  | -         | -        | -        
large string 1 MiB   | n=1   | -         | -         | -        | -        
large string 1 MiB   | n=10  | -         | -         | -        | -        
large string 1 MiB   | n=100 | -         | -         | -        | -        
Uint8Array 1 MiB     | n=1   | 105.50 us | 550.03 us | 1.33 ms  | 657.28 us
Uint8Array 1 MiB     | n=10  | 4.29 ms   | 4.14 ms   | 4.59 ms  | 6.00 ms  
Uint8Array 1 MiB     | n=100 | 37.72 ms  | 31.28 ms  | 47.63 ms | 55.18 ms 
```

## Batch P99 Latency (less is better)

```text
benchmark            | batch | tokio     | bun      | node     | deno    
---------------------+-------+-----------+----------+----------+---------
number f64 (8 bytes) | n=1   | 7.41 us   | -        | -        | -       
number f64 (8 bytes) | n=10  | 14.43 us  | -        | -        | -       
number f64 (8 bytes) | n=100 | 82.24 us  | -        | -        | -       
large string 1 MiB   | n=1   | -         | -        | -        | -       
large string 1 MiB   | n=10  | -         | -        | -        | -       
large string 1 MiB   | n=100 | -         | -        | -        | -       
Uint8Array 1 MiB     | n=1   | 170.53 us | 3.18 ms  | 2.50 ms  | 3.57 ms 
Uint8Array 1 MiB     | n=10  | 4.85 ms   | 6.56 ms  | 10.27 ms | 17.50 ms
Uint8Array 1 MiB     | n=100 | 44.28 ms  | 49.51 ms | 63.96 ms | 70.44 ms
```

## Avg Ratio Vs Tokio

```text
benchmark            | batch | bun/tokio | node/tokio | deno/tokio
---------------------+-------+-----------+------------+-----------
number f64 (8 bytes) | n=1   | -         | -          | -         
number f64 (8 bytes) | n=10  | -         | -          | -         
number f64 (8 bytes) | n=100 | -         | -          | -         
large string 1 MiB   | n=1   | -         | -          | -         
large string 1 MiB   | n=10  | -         | -          | -         
large string 1 MiB   | n=100 | -         | -          | -         
Uint8Array 1 MiB     | n=1   | 5.21x     | 12.61x     | 6.23x     
Uint8Array 1 MiB     | n=10  | 0.97x     | 1.07x      | 1.40x     
Uint8Array 1 MiB     | n=100 | 0.83x     | 1.26x      | 1.46x     
```

## Uint8Array Size Sweep Avg Latency (less is better)

```text
size    | tokio     | bun       | node      | deno     
--------+-----------+-----------+-----------+----------
8 B     | 73.62 us  | 69.20 us  | 78.98 us  | 116.08 us
16 B    | 70.72 us  | 60.20 us  | 58.65 us  | 78.47 us 
32 B    | 71.81 us  | 48.90 us  | 57.59 us  | 75.73 us 
64 B    | 75.49 us  | 70.67 us  | 58.68 us  | 72.82 us 
128 B   | 83.21 us  | 62.09 us  | 60.66 us  | 74.66 us 
256 B   | 75.49 us  | 52.58 us  | 66.71 us  | 74.05 us 
512 B   | 74.84 us  | 73.11 us  | 70.39 us  | 103.74 us
1 KiB   | 78.73 us  | 172.66 us | 115.37 us | 156.04 us
2 KiB   | 144.89 us | 227.89 us | 146.01 us | 307.85 us
4 KiB   | 145.22 us | 229.93 us | 260.83 us | 455.26 us
8 KiB   | 139.63 us | 316.36 us | 462.04 us | 614.61 us
16 KiB  | 183.35 us | 530.75 us | 670.49 us | 1.01 ms  
32 KiB  | 1.04 ms   | 868.04 us | 980.54 us | 1.72 ms  
64 KiB  | 2.07 ms   | 1.58 ms   | 1.86 ms   | 2.46 ms  
128 KiB | 4.08 ms   | 3.32 ms   | 4.79 ms   | 5.31 ms  
256 KiB | 8.61 ms   | 6.81 ms   | 10.03 ms  | 12.49 ms 
512 KiB | 17.85 ms  | 15.54 ms  | 20.93 ms  | 25.35 ms 
1 MiB   | 37.00 ms  | 33.13 ms  | 46.62 ms  | 54.42 ms 
```

## Arc Comparison Size Sweep Avg Latency (less is better)

Tokio uses `Arc<Vec<u8>>` here as a separate shared-bytes reference point, not the default apples-to-apples byte path.

```text
size  | tokio    | bun      | node      | deno     
------+----------+----------+-----------+----------
8 B   | 59.18 us | 81.52 us | 136.12 us | 119.92 us
16 B  | 64.91 us | 63.31 us | 83.94 us  | 84.22 us 
32 B  | 66.22 us | 61.01 us | 77.62 us  | 75.87 us 
64 B  | 56.23 us | 55.52 us | 71.72 us  | 77.14 us 
128 B | 55.91 us | 62.54 us | 65.93 us  | 79.93 us 
256 B | 55.98 us | 57.70 us | 67.35 us  | 79.94 us 
512 B | 54.64 us | 74.64 us | 98.49 us  | 140.18 us
```

## Arc Comparison Avg Ratio Vs Tokio

```text
size  | bun/tokio | node/tokio | deno/tokio
------+-----------+------------+-----------
8 B   | 1.38x     | 2.30x      | 2.03x     
16 B  | 0.98x     | 1.29x      | 1.30x     
32 B  | 0.92x     | 1.17x      | 1.15x     
64 B  | 0.99x     | 1.28x      | 1.37x     
128 B | 1.12x     | 1.18x      | 1.43x     
256 B | 1.03x     | 1.20x      | 1.43x     
512 B | 1.37x     | 1.80x      | 2.57x     
```
