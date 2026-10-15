import test from'node:test';import assert from'node:assert/strict';import{materialPassKey}from'./scenery-materials.js';
test('ordered passes keep the first material key and number later passes',()=>{
 assert.equal(materialPassKey('facade',0),'facade');assert.equal(materialPassKey('facade',1),'facade#pass2');assert.equal(materialPassKey('facade',7),'facade#pass8');
});
