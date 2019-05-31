mod cs_events;
mod ffigen;

fn main() {
	ffigen::gen_events().unwrap();
}
