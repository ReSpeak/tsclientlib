use ts_cli_completer::*;

fn main() {
	while let Some(cmd) = read_command() {
		println!("{:?}", cmd);
	}
}
