mod audit;

fn main() -> anyhow::Result<()> {
    audit::run()
}
