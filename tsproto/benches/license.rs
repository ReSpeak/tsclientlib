use base64::prelude::*;
use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use tsproto::license::Licenses;
use tsproto_types::crypto::EccKeyPubEd25519;

fn license_parse(b: &mut Bencher, license: Vec<u8>) {
	b.iter(|| {
		Licenses::parse_ignore_expired(license.clone()).unwrap();
	});
}

fn license_derive_key(b: &mut Bencher, license: Vec<u8>) {
	let licenses = Licenses::parse_ignore_expired(license).unwrap();

	b.iter(|| {
		let derived_key =
			licenses.derive_public_key(EccKeyPubEd25519::from_bytes(tsproto::ROOT_KEY)).unwrap();
		derived_key.compress().0
	});
}

const STANDARD_LICENSE: &str = "AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
		Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
		oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==";

const AAL_LICENSE: &str = "AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lY\
		TNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAABhl9gwla/UJ\
		p2Eszst9TRVXO/PeE6a6d+CTI6Pg7OEVgAJc5CrL4Nh8gAAACRUZWFtU3BlYWsgc3lz\
		dGVtcyBHbWJIAACvTQIgpv6zmLZq3znh7ygmOSokGFkFjz4bTigrOnetrgIJdIIACdS\
		/gAYAAAAAU29zc2VuU3lzdGVtcy5iaWQAADY7+uV1CQ1niOvYSdGzsu83kPTNWijovr\
		3B78eHGeePIAm98vQJvpu0";

fn standard_license_parse(b: &mut Bencher) {
	license_parse(b, BASE64_STANDARD.decode(STANDARD_LICENSE).unwrap());
}

fn aal_license_parse(b: &mut Bencher) {
	license_parse(b, BASE64_STANDARD.decode(AAL_LICENSE).unwrap());
}

fn standard_license_derive_key(b: &mut Bencher) {
	license_derive_key(b, BASE64_STANDARD.decode(STANDARD_LICENSE).unwrap());
}

fn aal_license_derive_key(b: &mut Bencher) {
	license_derive_key(b, BASE64_STANDARD.decode(AAL_LICENSE).unwrap());
}

fn bench_license(c: &mut Criterion) {
	c.bench_function("parse standard license", standard_license_parse);
	c.bench_function("parse aal license", aal_license_parse);

	c.bench_function("derive key standard license", standard_license_derive_key);
	c.bench_function("derive key aal license", aal_license_derive_key);
}

criterion_group!(benches, bench_license);
criterion_main!(benches);
