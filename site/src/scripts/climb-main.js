/* eslint-disable no-empty, no-redeclare, @typescript-eslint/no-unused-expressions, @typescript-eslint/no-unused-vars */
/* The body below is the prototype's script. Its short names and compact
   loops are the design file's own; rewriting them would break the verbatim
   contract, so the lint rules those idioms trip are disabled for this file
   only. climb-glass.js needs no override. */
/* climb-main.js — verbatim from prototype (docs/site/prototype/climb-site.html).
   Logic, constants and order are unchanged. The only edits are the data
   hand-off (T, the configuration captions and the card copy arrive in a JSON
   block), a guard for the shared layout, and the removal of SMIL: the ladder
   dot is now placed on its path by requestAnimationFrame. */

(function(){document.documentElement.classList.add('js');var bst=1;
var rm=matchMedia('(prefers-reduced-motion: reduce)').matches,$=function(s){return document.querySelector(s)},$$=function(s){return[].slice.call(document.querySelectorAll(s))};
var m=$('#mn'),nv=$('#nv');m.onclick=function(){var o=nv.classList.toggle('open');m.setAttribute('aria-expanded',o)};nv.onclick=function(e){if(e.target.tagName=='A'){nv.classList.remove('open');m.setAttribute('aria-expanded',false)}};
document.addEventListener('click',function(e){var b=e.target.closest('[data-c]');if(!b)return;try{navigator.clipboard.writeText(b.dataset.c);b.textContent='Copied';setTimeout(function(){b.textContent='Copy'},1400)}catch(x){}});
if(!document.getElementById('gc'))return;
/* ladder */
var DATA=JSON.parse(document.getElementById('climb-data').textContent),EV=DATA.evidence,T=DATA.explorer;
var lad=$('#lad'),sv=$('#sv'),cp=$('#cp'),lv=$('#lv'),dt=$('#dt'),rt=$('#rt'),NS='http://www.w3.org/2000/svg',ac=0,al=0,hc=-1,hl=-1,rev=0,pc=[],pl=[],ps2=[];
function esc(s){return s.replace(/&/g,'&amp;').replace(/</g,'&lt;')}
T.forEach(function(c,i){var b=document.createElement('button');b.className='c';b.style.setProperty('--i',i);b.innerHTML='<span>'+c[0]+'</span><b>'+c[1].length+'</b>';
 b.onclick=function(){ac=i;al=0;render(1)};b.onmouseenter=function(){hc=i;hv()};b.onmouseleave=function(){hc=-1;hv()};cp.appendChild(b)});
function hv(){pc.forEach(function(p,i){p.classList.toggle('hv',i==hc&&i!=ac)});pl.forEach(function(p,j){p.classList.toggle('hv',j==hl&&j!=al)})}
function render(a){[].forEach.call(cp.children,function(b,i){b.setAttribute('aria-pressed',i==ac)});lv.innerHTML='';hl=-1;
 T[ac][1].forEach(function(l,j){var b=document.createElement('button');b.className='l';b.setAttribute('aria-pressed',j==al);b.innerHTML='<span>'+l[0]+'</span><span class="ev '+l[1]+'">'+EV[l[1]]+'</span>';b.onclick=function(){al=j;render(0)};b.onmouseenter=function(){hl=j;hv()};b.onmouseleave=function(){hl=-1;hv()};lv.appendChild(b)});
 var l=T[ac][1][al];dt.innerHTML='<span class="cr">'+T[ac][0]+'</span><h3>'+l[0]+'</h3><span class="ev '+l[1]+'">'+EV[l[1]]+'</span><p>'+esc(l[2])+'</p><div class="cmd"><code>'+l[3]+'</code><button type="button" data-c="'+l[3]+'">Copy</button></div>';draw(a)}
function st(x1,y1,x2,y2){var m=(x1+x2)/2;return'M'+x1+' '+y1+'C'+m+' '+y1+' '+m+' '+y2+' '+x2+' '+y2}
var dotRaf=0;function travel(path,dot){cancelAnimationFrame(dotRaf);var L=path.getTotalLength(),t0=0;function step(now){t0=t0||now;var q=path.getPointAtLength(((now-t0)%2600)/2600*L);dot.setAttribute('cx',q.x);dot.setAttribute('cy',q.y);dotRaf=requestAnimationFrame(step)}dotRaf=requestAnimationFrame(step)}
function draw(a){cancelAnimationFrame(dotRaf);sv.innerHTML='';pc=[];pl=[];ps2=[];if(innerWidth<=900)return;var o=lad.getBoundingClientRect();
 function pt(el,s){var r=el.getBoundingClientRect();return[(s?r.right:r.left)-o.left,r.top+r.height/2-o.top]}
 function add(p1,p2,on,an,dl){var p=document.createElementNS(NS,'path'),h=!rev||an,n=document.createElementNS(NS,'circle');p.setAttribute('d',st(p1[0],p1[1],p2[0],p2[1]));p.setAttribute('pathLength',1);if(on)p.classList.add('on');
  p.style.strokeDashoffset=h?1:0;p.style.transition='stroke-dashoffset 1.2s cubic-bezier(.5,0,.2,1) '+dl+'s,stroke .3s,stroke-width .3s';sv.appendChild(p);
  n.setAttribute('cx',p2[0]);n.setAttribute('cy',p2[1]);n.setAttribute('r',3.5);n.setAttribute('class','nd'+(on?' on':''));sv.appendChild(n);if(h)ps2.push(p);return p}
 var r=pt(rt,1);[].forEach.call(cp.children,function(b,i){pc.push(add(r,pt(b,0),i==ac,0,i*.08))});
 var c=pt(cp.children[ac],1);[].forEach.call(lv.children,function(b,j){pl.push(add(c,pt(b,0),j==al,a,.1+j*.1))});
 add(pt(lv.children[al],1),pt(dt,0),1,a,.4);
 if(!rm){var d=document.createElementNS(NS,'circle');d.setAttribute('r',3.5);d.setAttribute('class','pd');d.setAttribute('data-path',pc[ac].getAttribute('d'));sv.appendChild(d);travel(pc[ac],d)}
 void sv.getBoundingClientRect();if(rev)ps2.forEach(function(p){p.style.strokeDashoffset=0});hv()}
function revLad(){rev=1;ps2.forEach(function(p){p.style.strokeDashoffset=0})}
var D=DATA.capabilities,cds=$('#cds');
D.forEach(function(d,i){var b=document.createElement('button');b.className='cd';b.type='button';b.innerHTML='<span class="k">('+'abcdefg'[i]+')</span><h3>'+T[i][0]+'</h3><p>'+d[0]+'</p><span class="mo">'+d[1]+'</span>';b.onclick=function(){ac=i;al=0;render(1);$('#ladder').scrollIntoView({behavior:rm?'auto':'smooth'})};cds.appendChild(b)});
cds.insertAdjacentHTML('beforeend','<a class="cd or" href="https://github.com/aiori-hackathon/A3-PS036-TC529" rel="noopener"><span>All seven capabilities are in the open repository.</span><b>View on GitHub ↗</b></a>');
render(1);new ResizeObserver(function(){draw()}).observe(lad);
/* configurations */
var HW=DATA.configs,tabs=$$('[role=tab]'),bars=$$('.bar');
function sel(k,f){tabs.forEach(function(t,j){t.setAttribute('aria-selected',j==k);t.tabIndex=j==k?0:-1;bars[j].classList.toggle('on',j==k);$('#'+HW[j][3]).style.display=j!=k?'none':''});$('#ph').textContent=HW[k][0];$('#pp').textContent=HW[k][1];$('#gt').textContent=HW[k][2];if(f)tabs[k].focus()}
tabs.forEach(function(t,k){t.onclick=function(){sel(k)};t.onkeydown=function(e){if(e.key=='ArrowRight')sel((k+1)%3,1);if(e.key=='ArrowLeft')sel((k+2)%3,1)}});sel(0);
/* one reveal: figures count up, bars fill */
function count(el){var n=+el.dataset.n,d=+el.dataset.d,t0=0,f=function(t){t0=t0||t;var p=Math.min((t-t0)/1400,1);el.textContent=(n*(1-Math.pow(1-p,3))).toLocaleString('en',{minimumFractionDigits:d,maximumFractionDigits:d});if(p<1)requestAnimationFrame(f)};requestAnimationFrame(f)}
if(!rm){$$('.fill').forEach(function(b){b.style.width=0});$$('[data-n]').forEach(function(e){e.textContent='0'})}
new IntersectionObserver(function(es,io){es.forEach(function(e){if(!e.isIntersecting)return;io.unobserve(e.target);if(e.target.id=='bars')$$('.fill').forEach(function(b){b.style.width=b.dataset.w+'%'});else $$('[data-n]').forEach(rm?function(){}:count)})},{threshold:.25}).observe($('#bars'));
new IntersectionObserver(function(es,io){if(es[0].isIntersecting){io.disconnect();$$('[data-n]').forEach(function(e){if(rm)return;count(e)})}},{threshold:.2}).observe($('.led'));
if(rm)$$('.fill').forEach(function(b){b.style.width=b.dataset.w+'%'});
/* hero: calm grid ripple + nanoparticle orbits */
var hero=$('.hero'),gc=$('#gc'),g=gc.getContext('2d'),ob=$('#ob'),of=$('#of'),cb=ob.getContext('2d'),cf=of.getContext('2d'),lo=$('.lo');
var CELL=56,COL=[[255,84,0],[254,127,0],[255,166,166]],rip=[],ci=0,W=0,H=0,dpr=1,vis=false,last=0,nxt=0,raf=0,mx=-1e4,my=-1e4,V=560,TINT=['242,237,228','255,166,166','254,127,0'];
var RG=[{A:215,B:70,L:-22*Math.PI/180},{A:185,B:104,L:26*Math.PI/180}],ps=[];
for(var i=0;i<(innerWidth<700?360:720);i++){var q=Math.random();ps.push({g:i%2,t:Math.random()*6.283,v:.2+Math.random()*.4,s:1+(Math.random()-.5)*.34,r:.3+Math.random()*.45,a:.3+Math.random()*.55,c:TINT[q<.72?0:q<.88?1:2],ox:0,oy:0})}
function size(){dpr=Math.min(devicePixelRatio||1,2);var r=hero.getBoundingClientRect();W=r.width;H=r.height;gc.width=W*dpr;gc.height=H*dpr;gc.style.width=W+'px';gc.style.height=H+'px';var s=Math.round(of.getBoundingClientRect().width*dpr);ob.width=ob.height=of.width=of.height=s;if(!vis)frame(performance.now(),0)}
function add(x,y,a,t){rip.push({x:x,y:y,t:t||performance.now(),c:COL[ci++%3],a:a})}
function ctr(){var r=lo.getBoundingClientRect(),h=hero.getBoundingClientRect();return[r.left+r.width/2-h.left,r.top+r.height/2-h.top]}
function k(x,y,now){var b=0,c=COL[0];for(var i=0;i<rip.length;i++){var q=rip[i],age=(now-q.t)/1000,d=Math.hypot(x-q.x,y-q.y),u=(d-age*260)/70,v=Math.exp(-u*u)*(1-age/4)*q.a;if(v>b){b=v;c=q.c}}return[b,c]}
function frame(now,dt){g.setTransform(dpr,0,0,dpr,0,0);g.clearRect(0,0,W,H);rip=rip.filter(function(q){return now-q.t<4000});
 g.strokeStyle='rgba(242,237,228,.05)';g.lineWidth=1;g.beginPath();var x,y;for(x=0;x<=W;x+=CELL){g.moveTo(x+.5,0);g.lineTo(x+.5,H)}for(y=0;y<=H;y+=CELL){g.moveTo(0,y+.5);g.lineTo(W,y+.5)}g.stroke();
 if(rip.length)for(x=0;x<=W;x+=CELL)for(y=0;y<=H;y+=CELL){var h=k(x+CELL/2,y,now),v=k(x,y+CELL/2,now),n=k(x,y,now);
  if(h[0]>.05){g.strokeStyle='rgba('+h[1]+','+Math.min(.5,h[0]*.5)+')';g.beginPath();g.moveTo(x,y+.5);g.lineTo(x+CELL,y+.5);g.stroke()}
  if(v[0]>.05){g.strokeStyle='rgba('+v[1]+','+Math.min(.5,v[0]*.5)+')';g.beginPath();g.moveTo(x+.5,y);g.lineTo(x+.5,y+CELL);g.stroke()}
  if(n[0]>.08){g.fillStyle='rgba('+n[1]+','+Math.min(.7,n[0]*.7)+')';g.fillRect(x-1,y-1,2,2)}}
 var s=of.width/V,rc=of.getBoundingClientRect(),mvx=(mx-rc.left)/rc.width*V,mvy=(my-rc.top)/rc.height*V,R=84;
 [cb,cf].forEach(function(c){c.setTransform(1,0,0,1,0,0);c.clearRect(0,0,of.width,of.height);c.setTransform(s,0,0,s,0,0)});
 RG.forEach(function(Q){cb.strokeStyle='rgba(242,237,228,.07)';cb.lineWidth=.8;cb.beginPath();cb.ellipse(280,280,Q.A,Q.B,Q.L,Math.PI,2*Math.PI);cb.stroke();cf.strokeStyle='rgba(242,237,228,.16)';cf.lineWidth=.8;cf.beginPath();cf.ellipse(280,280,Q.A,Q.B,Q.L,0,Math.PI);cf.stroke()});
 ps.forEach(function(p){var Q=RG[p.g],ca=Math.cos(Q.L),sa=Math.sin(Q.L);p.t+=p.v*dt*bst;var sn=Math.sin(p.t),ex=Q.A*p.s*Math.cos(p.t),ey=Q.B*p.s*sn,px=280+ex*ca-ey*sa,py=280+ex*sa+ey*ca,tx=0,ty=0,dx=px-mvx,dy=py-mvy,d=Math.hypot(dx,dy);
  if(d<R&&d>.01){var f=(1-d/R);tx=dx/d*f*11;ty=dy/d*f*11}p.ox+=(tx-p.ox)*.12;p.oy+=(ty-p.oy)*.12;
  var fr=sn>0,c=fr?cf:cb,r=p.r*(fr?1:.8);c.fillStyle='rgba('+p.c+','+(p.a*(fr?1:.5)).toFixed(2)+')';c.fillRect(px+p.ox-r,py+p.oy-r,r*2,r*2)})}
function loop(now){if(!vis)return;var dt=Math.min((now-last)/1000||.016,.05);last=now;if(now>nxt){var c=ctr();add(c[0],c[1],.8,now);nxt=now+5200}frame(now,dt);raf=requestAnimationFrame(loop)}
size();new ResizeObserver(size).observe(hero);
if(rm){var c0=ctr();add(c0[0],c0[1],.8,performance.now()-1200);frame(performance.now(),0)}
else{addEventListener('pointermove',function(e){mx=e.clientX;my=e.clientY},{passive:true});document.addEventListener('pointerleave',function(){mx=my=-1e4});
 hero.addEventListener('pointerdown',function(e){if(e.target.closest('a,button'))return;var r=hero.getBoundingClientRect();add(e.clientX-r.left,e.clientY-r.top,.6)});
 new IntersectionObserver(function(es){vis=es[0].isIntersecting;cancelAnimationFrame(raf);if(vis){last=performance.now();raf=requestAnimationFrame(loop)}}).observe(hero)}

/* scroll: reveal, scale index, nav spy, climbing particles */
var secs=$$('main section'),LB=DATA.sections,rl=$('#rl'),ix=$('#ix'),pr=$('#pr'),pb=$('#pb'),nl=$$('#nv a[href^="#"]'),N=72,tk=[],lb=[],mj=[],ticking=0;
for(var i=0;i<=N;i++){var t=document.createElement('i');t.style.top=i/N*100+'%';rl.appendChild(t);tk.push(t)}
secs.forEach(function(s,i){var a=document.createElement('a');a.href='#'+(s.id||'top');a.textContent=LB[i];rl.appendChild(a);lb.push(a)});
function lay(){var mxs=Math.max(1,document.documentElement.scrollHeight-innerHeight);secs.forEach(function(s,i){lb[i].style.top=Math.min(1,(s.getBoundingClientRect().top+scrollY)/mxs)*100+'%'});
 mj=tk.map(function(t,i){return secs.some(function(s){return Math.abs(Math.min(1,(s.getBoundingClientRect().top+scrollY)/mxs)*N-i)<.5})});tk.forEach(function(t,i){t.classList.toggle('mj',mj[i])});upd()}
function upd(){var mxs=Math.max(1,document.documentElement.scrollHeight-innerHeight),p=Math.min(1,scrollY/mxs),cur=0;
 secs.forEach(function(s,i){if(s.getBoundingClientRect().top<innerHeight*.4)cur=i});
 tk.forEach(function(t,i){var e=(i/N-p)*N;t.style.width=((mj[i]?16:7)+20*Math.exp(-e*e/9))+'px'});
 ix.style.top=p*100+'%';pr.textContent=Math.round(p*100)+'%';pb.style.transform='scaleX('+p+')';
 lb.forEach(function(a,i){a.classList.toggle('on',i==cur)});nl.forEach(function(a){a.classList.toggle('on',a.getAttribute('href')=='#'+secs[cur].id)})}
addEventListener('scroll',function(){if(!ticking){ticking=1;requestAnimationFrame(function(){ticking=0;upd()})}},{passive:true});addEventListener('resize',lay);addEventListener('load',lay);setTimeout(lay,800);lay();
$$('.sh p,.led li,.key,.note,.tr>div,.lim,.lk,.tm li,.hw>*,.arch,.fg>div,.fb,.cd,.gs,.kw li,.mt,.ld,.cta').forEach(function(e){e.classList.add('r');e.style.setProperty('--d',[].indexOf.call(e.parentNode.children,e))});
$$('h1,h2').forEach(function(e){e.classList.add('cl');e.innerHTML='<span class="cs">'+e.innerHTML+'</span>'});
var fx=$('#fx'),f=fx.getContext('2d'),FW=0,FH=0,fp=[],bu=[],sy=scrollY,sv2=0;
function fz(){FW=fx.width=innerWidth;FH=fx.height=innerHeight}fz();addEventListener('resize',fz);
for(var i=0;i<(innerWidth<700?60:130);i++)fp.push({x:Math.random()*innerWidth,y:Math.random()*innerHeight,z:.25+Math.random()*.75,a:.12+Math.random()*.35,o:Math.random()<.18});
function burst(x,y){if(rm)return;for(var i=0;i<34;i++){var a=Math.random()*6.283,s=.5+Math.random()*2.2;bu.push({x:x,y:y,vx:Math.cos(a)*s,vy:Math.sin(a)*s-1,l:1,s:1+Math.random()*1.4,c:TINT[Math.random()<.6?2:0]})}}
new IntersectionObserver(function(es,io){es.forEach(function(e){if(!e.isIntersecting)return;var t=e.target;t.classList.add('in');io.unobserve(t);if(t.id=='lad')revLad();if(t.tagName=='H2'){var r=t.getBoundingClientRect();burst(r.left+Math.min(r.width,260)/2,r.top+r.height/2)}})},{threshold:.15,rootMargin:'0px 0px -6% 0px'}).observe&&(function(io){$$('.r,.cl,#lad').forEach(function(e){io.observe(e)})})(new IntersectionObserver(function(es,io){es.forEach(function(e){if(!e.isIntersecting)return;var t=e.target;t.classList.add('in');io.unobserve(t);if(t.id=='lad')revLad();if(t.tagName=='H2'){var r=t.getBoundingClientRect();burst(r.left+Math.min(r.width,260)/2,r.top+r.height/2)}})},{threshold:.15,rootMargin:'0px 0px -6% 0px'}));
function fl(){var y=scrollY;sv2+=((y-sy)-sv2)*.12;sy=y;bst=1+Math.min(Math.abs(sv2)*.06,3);f.clearRect(0,0,FW,FH);
 fp.forEach(function(p){p.y-=p.z*(.18+sv2*.35);if(p.y<-6)p.y=FH+6;if(p.y>FH+6)p.y=-6;var s=p.o?1.6:1;f.fillStyle=p.o?'rgba(254,127,0,'+p.a+')':'rgba(242,237,228,'+p.a+')';f.fillRect(p.x,p.y,s,s+Math.abs(sv2)*p.z*.6)});
 bu=bu.filter(function(b){return b.l>0});bu.forEach(function(b){b.x+=b.vx;b.y+=b.vy;b.vy-=.02;b.vx*=.98;b.l-=.012;f.fillStyle='rgba('+b.c+','+(b.l*.8).toFixed(2)+')';f.fillRect(b.x,b.y,b.s,b.s)});requestAnimationFrame(fl)}
if(!rm)fl();

/* section bands: 0 = hashes climbing a Merkle tree, 1 = packets hopping a ladder */
function band(cv,mode){var g=cv.getContext('2d'),W=0,H=0,dp=1,pk=[],lit=[],xs=[],ys=[],lv=3,pa=[],vis=false,last=0,nx=0,raf=0,SP=46;
 function bt(){var n=W<520?8:16,l,i;lv=Math.log2(n);xs=[];ys=[];lit=[];for(l=0;l<=lv;l++){xs[l]=[];lit[l]=[];ys[l]=H-14-l*(H-34)/lv;for(i=0;i<(n>>l);i++){xs[l][i]=l?(xs[l-1][2*i]+xs[l-1][2*i+1])/2:(i+.5)*W/n;lit[l][i]=0}}
  pa=[];for(i=0;i<n;i++){var P=[[xs[0][i],ys[0]]],cu=[0],nd=[0],d=0,ix=i;for(l=1;l<=lv;l++){ix>>=1;var a=P[P.length-1],ym=(a[1]+ys[l])/2;[[a[0],ym],[xs[l][ix],ym],[xs[l][ix],ys[l]]].forEach(function(q){var p=P[P.length-1];d+=Math.abs(q[0]-p[0])+Math.abs(q[1]-p[1]);P.push(q);cu.push(d)});nd.push(d)}pa.push({P:P,cu:cu,nd:nd,L:d})}}
 function at(p,d){for(var i=1;i<p.P.length;i++)if(d<=p.cu[i]){var u=(d-p.cu[i-1])/(p.cu[i]-p.cu[i-1]||1);return[p.P[i-1][0]+(p.P[i][0]-p.P[i-1][0])*u,p.P[i-1][1]+(p.P[i][1]-p.P[i-1][1])*u]}return p.P[p.P.length-1]}
 function ry(r){return H*(.26+r*.24)}
 function sz(){dp=Math.min(devicePixelRatio||1,2);var r=cv.getBoundingClientRect();W=r.width;H=r.height;cv.width=W*dp;cv.height=H*dp;if(!mode)bt();pk=[];draw()}
 function step(now,dt){if(now>nx){nx=now+(mode?650:380);if(pk.length<8)pk.push(mode?{x:-8,y:ry(1),r:1,tr:1,k:-1,sp:70+Math.random()*50,t:[]}:{i:Math.random()*pa.length|0,d:0})}
  pk=pk.filter(function(p){if(!mode){p.d+=dt*110;for(var l=0;l<=lv;l++)if(p.d>=pa[p.i].nd[l])lit[l][p.i>>l]=1;return p.d<pa[p.i].L+14}
   var ty=ry(p.tr),dy=ty-p.y;if(Math.abs(dy)>.5)p.y+=Math.sign(dy)*Math.min(Math.abs(dy),dt*150);else{p.x+=p.sp*dt;var k=Math.floor(p.x/SP);if(k!=p.k){p.k=k;p.x=k*SP;if(Math.random()<.45)p.tr=Math.max(0,Math.min(2,p.tr+(Math.random()<.5?-1:1)))}}
   p.t.push([p.x,p.y]);if(p.t.length>30)p.t.shift();return p.x<W+20});
  if(!mode)lit.forEach(function(a){for(var i=0;i<a.length;i++)a[i]=Math.max(0,a[i]-dt*1.1)})}
 function draw(){g.setTransform(dp,0,0,dp,0,0);g.clearRect(0,0,W,H);var l,i;
  if(!mode){g.strokeStyle='rgba(242,237,228,.11)';g.lineWidth=1;g.beginPath();for(l=1;l<=lv;l++)for(i=0;i<xs[l].length;i++){var ym=(ys[l-1]+ys[l])/2;[2*i,2*i+1].forEach(function(c){g.moveTo(xs[l-1][c],ys[l-1]);g.lineTo(xs[l-1][c],ym);g.lineTo(xs[l][i],ym);g.lineTo(xs[l][i],ys[l])})}g.stroke();
   for(l=0;l<=lv;l++)for(i=0;i<xs[l].length;i++){var v=lit[l][i],s=3+l*1.2;g.fillStyle=v>.03?'rgba(255,84,0,'+(.35+.65*v)+')':'rgba(242,237,228,.24)';g.fillRect(xs[l][i]-s/2,ys[l]-s/2,s,s)}
   pk.forEach(function(p){for(var k=0;k<9;k++){var q=at(pa[p.i],Math.max(0,p.d-k*6));g.fillStyle='rgba(255,166,166,'+(.9*(1-k/9)).toFixed(2)+')';g.fillRect(q[0]-1,q[1]-1,2,2)}})}
  else{g.strokeStyle='rgba(242,237,228,.13)';g.lineWidth=1;g.beginPath();for(i=0;i<3;i++){g.moveTo(0,ry(i)+.5);g.lineTo(W,ry(i)+.5)}for(var x=SP;x<W;x+=SP){g.moveTo(x+.5,ry(0));g.lineTo(x+.5,ry(2))}g.stroke();
   pk.forEach(function(p){var n=p.t.length;p.t.forEach(function(q,j){g.fillStyle=(j%3?'rgba(255,166,166,':'rgba(254,127,0,')+(j/n*.85).toFixed(2)+')';g.fillRect(q[0]-1,q[1]-1,2,2)});g.fillStyle='#ff5400';g.fillRect(p.x-2,p.y-2,4,4)})}}
 function loop(now){if(!vis)return;var dt=Math.min((now-last)/1000||.016,.05);last=now;step(now,dt);draw();raf=requestAnimationFrame(loop)}
 sz();new ResizeObserver(sz).observe(cv);if(rm)return;
 new IntersectionObserver(function(es){vis=es[0].isIntersecting;cancelAnimationFrame(raf);if(vis){last=performance.now();raf=requestAnimationFrame(loop)}}).observe(cv)}
$$('.bd canvas').forEach(function(c){band(c,+c.dataset.m)});
})();
