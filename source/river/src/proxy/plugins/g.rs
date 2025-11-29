use wasmtime::component::bindgen;
bindgen!({
    world: "filter-world",
    path: "./wit/request.wit",
});