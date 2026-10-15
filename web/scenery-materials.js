// Ordered PSM passes are split and read in Rust (scenery_material::describe).
// Each pass after the first keeps its own texture-map key.
export function materialPassKey(name,index) {return index===0?name:`${name}#pass${index+1}`;}
