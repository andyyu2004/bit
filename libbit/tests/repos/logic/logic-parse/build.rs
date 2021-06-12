fn main() {
    lalrpop::Configuration::default().generate_in_source_tree().process().unwrap()
}
