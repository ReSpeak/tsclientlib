use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use num_bigint::BigUint;
use num_traits::One;
#[cfg(feature = "rug")]
use rug::Integer;

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

#[cfg(feature = "rug")]
fn gmp_modpow(b: &mut Bencher) {
	let n = "9387019355706217197639129234358945126657617361248696932841794255538327365072557602175160199263073329488914880215590036563068284078359088114486271428098753";
	let x = "2148617454765635492758175407769288127281667975788420713054995716016550287184632946544163990319181591625774561067011999700977775946073267145316355582522577";
	let level = 10_000;
	let n: Integer = n.parse().unwrap();
	let x: Integer = x.parse().unwrap();
	let mut e = Integer::new();
	e.set_bit(level, true);

	b.iter(|| x.pow_mod_ref(&e, &n).unwrap());
}

fn bench_modpow(c: &mut Criterion) {
	c.bench_function("num bigint", num_modpow);

	#[cfg(feature = "rug")]
	{
		c.bench_function("gmp", gmp_modpow);
	}
}

criterion_group!(benches, bench_modpow);
criterion_main!(benches);
