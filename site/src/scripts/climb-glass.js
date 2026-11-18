/* climb-glass.js — verbatim from prototype (docs/site/prototype/climb-site.html).
   The pointer-tracked specular light is unchanged. Two guards were added, both
   required by the reduced-motion and lite-mode rules: no listener is attached
   when the reader asks for less motion or a lighter page. */
import { lite } from '../lib/lite';

(function(){var rm=matchMedia('(prefers-reduced-motion: reduce)').matches;if(rm||lite())return;var de=document.documentElement;if(navigator.userAgentData)de.classList.add('rf');
var G='#hd,.led li,.tm li,.tr>div,.lim,.dg,.arch',raf=0,ev;
addEventListener('pointermove',function(e){ev=e;if(raf)return;raf=requestAnimationFrame(function(){raf=0;var t=ev.target,el=t&&t.closest&&t.closest(G);if(!el)return;var r=el.getBoundingClientRect();el.style.setProperty('--mx',(ev.clientX-r.left)+'px');el.style.setProperty('--my',(ev.clientY-r.top)+'px')})},{passive:true})})();
