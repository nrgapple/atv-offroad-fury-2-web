// Host-only integration proxy: approved static allowlist server plus local Rust relay.
// No private file access and no public tunnel. Bind is explicit.
import http from 'node:http';
const port=Number(process.env.PORT??4187), relay=Number(process.env.RELAY_PORT??8789), assets=Number(process.env.ASSET_PORT??4177);
const server=http.createServer((req,res)=>{
 const dest=req.url.startsWith('/multiplayer/')?relay:assets;
 const proxy=http.request({host:'127.0.0.1',port:dest,path:req.url,method:req.method,headers:req.headers},upstream=>{res.writeHead(upstream.statusCode,upstream.headers);upstream.pipe(res);});
 proxy.on('error',()=>{if(!res.headersSent)res.writeHead(502);res.end('Host integration upstream unavailable');});req.pipe(proxy);
});
server.on('upgrade',(req,socket,head)=>{
 if(!req.url.startsWith('/multiplayer/rooms/')){socket.destroy();return;}
 const proxy=http.request({host:'127.0.0.1',port:relay,path:req.url,method:req.method,headers:req.headers});
 proxy.on('upgrade',(response,upstream,first)=>{
  socket.write(`HTTP/1.1 ${response.statusCode} Switching Protocols\r\n`+Object.entries(response.headers).map(([key,value])=>`${key}: ${value}\r\n`).join('')+'\r\n');
  if(first.length)socket.write(first);if(head.length)upstream.write(head);socket.pipe(upstream);upstream.pipe(socket);
  upstream.on('error',()=>socket.destroy());socket.on('error',()=>upstream.destroy());socket.on('close',()=>upstream.destroy());
 });proxy.on('response',response=>{socket.write(`HTTP/1.1 ${response.statusCode} Rejected\r\nConnection: close\r\n\r\n`);socket.end();});proxy.on('error',()=>socket.destroy());proxy.end();
});
server.listen(port,process.env.BIND??'0.0.0.0',()=>console.log(`Host-only multiplayer test proxy on port ${port}`));
