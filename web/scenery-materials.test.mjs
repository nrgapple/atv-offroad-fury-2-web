import test from'node:test';import assert from'node:assert/strict';import{materialPassSources,materialPassKey}from'./scenery-materials.js';
test('ordered reflection/diffuse passes retain their own bindings and shared declarations',()=>{
 const text='[General]\nTotalShaderPasses=2\nEnableMultiPassCull=0\n[ShaderPass_1]\nmicroProgramName=PrelitReflXZEnvmapCompressed\nTextureMap_1=sky\n[ShaderPass_2];source overlay\nmicroProgramName=diffuseCompressed\nTextureMap_1=facade\nalphaBlendingOnOff=1\n[Epilogue]\nDummy=1';
 const [reflection,diffuse]=materialPassSources(text);assert.match(reflection,/TextureMap_1=sky/);assert.doesNotMatch(reflection,/facade/);assert.match(diffuse,/\[ShaderPass_1\]/);assert.match(diffuse,/TextureMap_1=facade/);assert.doesNotMatch(diffuse,/PrelitRefl/);for(const source of[reflection,diffuse]){assert.match(source,/TotalShaderPasses=1/);assert.match(source,/EnableMultiPassCull=0/);assert.match(source,/\[Epilogue\]/);}assert.equal(materialPassKey('facade',0),'facade');assert.equal(materialPassKey('facade',1),'facade#pass2');
});
test('bounded pass declarations reject missing/duplicate sections and retain texture-only fallback',()=>{
 assert.deepEqual(materialPassSources(''),['']);for(const text of['[General]\nTotalShaderPasses=9','[General]\nTotalShaderPasses=2\n[ShaderPass_1]','[ShaderPass_1]\n[ShaderPass_1]','[General]\nTotalShaderPasses=NaN'])assert.throws(()=>materialPassSources(text));
});
