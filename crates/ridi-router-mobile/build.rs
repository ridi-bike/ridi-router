fn main() {
    uniffi::generate_scaffolding("src/ridi_router_mobile.udl").expect("failed to generate UniFFI scaffolding");
}
