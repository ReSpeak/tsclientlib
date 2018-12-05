#[macro_use]
extern crate criterion;
extern crate gmp;
extern crate num;
extern crate num_traits;

use criterion::{Bencher, Criterion, Fun};
use gmp::mpz::Mpz;
use num::bigint::BigUint;
use num_traits::One;

fn num_modpow(b: &mut Bencher) {
	let n = "9387019355706217197639129234358945126657617361248696932841794255538327365072557602175160199263073329488914880215590036563068284078359088114486271428098753";
	let x = "2148617454765635492758175407769288127281667975788420713054995716016550287184632946544163990319181591625774561067011999700977775946073267145316355582522577";
	let level = 10_000;
	let n = n.parse().unwrap();
	let x: BigUint = x.parse().unwrap();
	let mut e = BigUint::one();
	e <<= level as usize;

	b.iter(|| x.modpow(&e, &n));
}

fn gmp_modpow(b: &mut Bencher) {
	let n = "9387019355706217197639129234358945126657617361248696932841794255538327365072557602175160199263073329488914880215590036563068284078359088114486271428098753";
	let x = "2148617454765635492758175407769288127281667975788420713054995716016550287184632946544163990319181591625774561067011999700977775946073267145316355582522577";
	let level = 10_000;
	let n = n.parse().unwrap();
	let x: Mpz = x.parse().unwrap();
	let mut e = Mpz::new();
	e.setbit(level);

	b.iter(|| x.powm(&e, &n));
}

fn bench_modpow(c: &mut Criterion) {
	let num_mp = Fun::new("num bigint", |b, ()| num_modpow(b));
	let gmp_mp = Fun::new("gmp", |b, ()| gmp_modpow(b));
	c.bench_functions("ModPow", vec![num_mp, gmp_mp], ());
}

criterion_group!(benches, bench_modpow);
criterion_main!(benches);
