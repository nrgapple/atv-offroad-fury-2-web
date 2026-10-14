// Keep original ordered PSM passes. Each worker parse receives that pass's
// authored fields under ShaderPass_1; prologue/epilogue execution is separate.
export function materialPassSources(text) {
  if(text.length>1024*1024)throw new Error('Original material exceeds extent limit.');
  const sections=[];let current={header:'',lines:[]};sections.push(current);
  for(const line of text.split(/\r?\n/)){
    const match=line.split(';')[0].trim().match(/^\[([^\]]+)\]$/);
    if(match){current={header:match[1],lines:[]};sections.push(current);}else current.lines.push(line);
  }
  const declaration=sections.filter(s=>/^General$/i.test(s.header)).flatMap(s=>s.lines).map(l=>l.split(';')[0].trim()).find(l=>/^TotalShaderPasses\s*=/i.test(l));
  const count=declaration?Number(declaration.split('=')[1].trim()):1;
  if(!Number.isInteger(count)||count<1||count>8)throw new Error('Unsupported original material pass count.');
  const shared=sections.filter(s=>!/^ShaderPass_\d+$/i.test(s.header)).map(s=>(s.header?`[${s.header}]\n`:'')+s.lines.map(l=>/^\s*TotalShaderPasses\s*=/i.test(l)?'TotalShaderPasses=1':l).join('\n')).join('\n');
  if(!text.trim())return [''];
  return Array.from({length:count},(_,i)=>{
    const matches=sections.filter(s=>s.header.toLowerCase()===`shaderpass_${i+1}`);
    if(matches.length!==1)throw new Error(`Original material pass${i+1} is missing or duplicated.`);
    return `${shared}\n[ShaderPass_1]\n${matches[0].lines.join('\n')}`;
  });
}
export function materialPassKey(name,index) {return index===0?name:`${name}#pass${index+1}`;}
