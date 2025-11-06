const render = import("./pkg/render");

render.then(code=>{
    console.log(code.test_wasm());
}).catch(console.error);
