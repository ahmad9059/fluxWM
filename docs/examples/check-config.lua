-- Documentation fixture validator, not a hardened config evaluator.
local function fail(p, msg) error(p .. ': ' .. msg, 0) end
local function typ(v, t, p) if type(v) ~= t then fail(p, 'expected ' .. t) end end
local function num(v,p,lo,hi)
  typ(v,'number',p)
  if v ~= v or v == math.huge or v == -math.huge or v < lo or v > hi then fail(p,'out of bounds/nonfinite') end
end
local function rec(v, keys, p)
  typ(v,'table',p)
  for k in pairs(v) do if type(k) ~= 'string' or not keys[k] then fail(p,'unknown field '..tostring(k)) end end
end
local function fields(names)
  local r={} for s in names:gmatch('%S+') do r[s]=true end return r
end
local function array(v,p)
  typ(v,'table',p)
  local n=0 for k in pairs(v) do if type(k)~='number' or k%1~=0 or k<1 then fail(p,'not a dense array') end n=n+1 end
  if n~=#v then fail(p,'sparse array') end
end
local function enum(v, options,p) if not fields(options)[v] then fail(p,'invalid enum') end end
local function width(v,p,preset_allowed)
  rec(v,fields('fraction logical_px preset'),p)
  local n=0 for _ in pairs(v) do n=n+1 end if n~=1 then fail(p,'exactly one width tag required') end
  if v.fraction then num(v.fraction,p..'.fraction',0.01,16)
  elseif v.logical_px then num(v.logical_px,p..'.logical_px',1,65536)
  elseif preset_allowed and v.preset then num(v.preset,p..'.preset',1,128); if v.preset%1~=0 then fail(p,'preset must be integer') end
  else fail(p,'invalid width tag') end
end
local config=assert(loadfile('docs/examples/config.lua','t',{}))()
rec(config,fields('schema_version layout input outputs animations decoration bindings rules autostart security'),'config')
assert(config.schema_version==1)
local l=config.layout
rec(l,fields('mode gaps outer_gap width_presets default_width focus_margin canvas'),'layout')
enum(l.mode,'strip canvas','layout.mode')
for _,k in ipairs({'gaps','outer_gap','focus_margin'}) do num(l[k],'layout.'..k,0,4096) end
array(l.width_presets,'layout.width_presets')
for i,v in ipairs(l.width_presets) do width(v,'layout.width_presets['..i..']',false) end
width(l.default_width,'layout.default_width',true)
if l.default_width.preset and not l.width_presets[l.default_width.preset] then fail('layout.default_width','unknown preset') end
rec(l.canvas,fields('snap snap_distance_physical_px insertion'),'layout.canvas')
typ(l.canvas.snap,'boolean','canvas.snap');num(l.canvas.snap_distance_physical_px,'canvas.snap_distance_physical_px',0,512)
enum(l.canvas.insertion,'right left up down','canvas.insertion')
rec(config.input,fields('keymap pointer touchpad gestures'),'input')
rec(config.input.keymap,fields('layout options'),'input.keymap')
rec(config.input.pointer,fields('focus_follows_mouse acceleration'),'input.pointer')
rec(config.input.touchpad,fields('tap natural_scroll'),'input.touchpad')
rec(config.input.gestures,fields('pan_fingers overview_fingers'),'input.gestures')
array(config.outputs,'outputs')
for i,o in ipairs(config.outputs) do
  local p='outputs['..i..']';rec(o,fields('match scale adaptive_sync'),p)
  rec(o.match,fields('any connector make model serial'),p..'.match')
  local count=0 for k,v in pairs(o.match) do count=count+1;typ(v,k=='any' and 'boolean' or 'string',p..'.match.'..k) end
  if count==0 or (o.match.any~=nil and (o.match.any~=true or count~=1)) then fail(p,'invalid output matcher') end
  num(o.scale,p..'.scale',0.25,8)
  if o.adaptive_sync then enum(o.adaptive_sync,'auto on off',p..'.adaptive_sync') end
end
rec(config.animations,fields('enabled reduced_motion curves camera window fade'),'animations')
for k,curve in pairs(config.animations.curves) do
  if curve.type=='spring' then
    rec(curve,fields('type mass stiffness damping'),'curve.'..k)
    num(curve.mass,'mass',0.001,100);num(curve.stiffness,'stiffness',0.001,100000);num(curve.damping,'damping',0,100000)
  elseif curve.type=='bezier' then
    rec(curve,fields('type points'),'curve.'..k);array(curve.points,'points');assert(#curve.points==4)
    for i,v in ipairs(curve.points) do num(v,'point',i%2==1 and 0 or -4,i%2==1 and 1 or 4) end
  else fail('curve.'..k,'unknown type') end
end
for _,name in ipairs({'camera','window','fade'}) do
  local t=config.animations[name];rec(t,fields('curve duration_ms'),'animations.'..name)
  if not config.animations.curves[t.curve] then fail(name,'unknown curve') end
  if t.duration_ms then num(t.duration_ms,name..'.duration_ms',0,10000) end
end
rec(config.decoration,fields('border corner_radius shadow blur'),'decoration')
rec(config.decoration.border,fields('width active inactive'),'decoration.border')
rec(config.decoration.shadow,fields('enabled radius opacity'),'decoration.shadow')
rec(config.decoration.blur,fields('enabled radius passes'),'decoration.blur')
array(config.bindings,'bindings');local seen={}
local actions={spawn='name argv',focus='name direction',move='name direction',['close-window']='name',fullscreen='name toggle',overview='name toggle',['workspace-mode']='name mode',['reload-config']='name',lock='name'}
for i,b in ipairs(config.bindings) do
  local p='bindings['..i..']';rec(b,fields('key action'),p);rec(b.key,fields('sym code modifiers'),p..'.key')
  if (b.key.sym~=nil)==(b.key.code~=nil) then fail(p,'exactly one key tag required') end
  local identity
  if b.key.sym then typ(b.key.sym,'string',p..'.key.sym');if b.key.modifiers then fail(p,'modifiers are embedded in sym chord') end;identity='sym:'..b.key.sym
  else num(b.key.code,p..'.code',1,767);array(b.key.modifiers,p..'.modifiers');identity='code:'..b.key.code..':'..table.concat(b.key.modifiers,'+') end
  if seen[identity] then fail(p,'duplicate chord') end seen[identity]=true
  local a=b.action;if not actions[a.name] then fail(p,'action outside example contract') end;rec(a,fields(actions[a.name]),p..'.action')
  if a.argv then array(a.argv,p..'.argv');for _,v in ipairs(a.argv) do typ(v,'string',p..'.argv');if v:find('\0',1,true) then fail(p,'NUL argv') end end end
  if a.direction then enum(a.direction,'left right up down',p..'.direction') end
  if a.mode then enum(a.mode,'strip canvas',p..'.mode') end
end
array(config.rules,'rules')
for i,r in ipairs(config.rules) do
  rec(r,fields('match set dynamic'),'rules['..i..']')
  rec(r.match,fields('app_id title is_dialog'),'rules.match');rec(r.set,fields('floating center_on_parent capture'),'rules.set')
  if r.set.capture then enum(r.set.capture,'allow deny','rules.set.capture') end
end
array(config.autostart,'autostart')
for i,a in ipairs(config.autostart) do rec(a,fields('argv restart_on_change'),'autostart['..i..']');array(a.argv,'autostart.argv');typ(a.restart_on_change,'boolean','restart_on_change') end
rec(config.security,fields('portal_capture ipc'),'security')
enum(config.security.portal_capture,'consent','security.portal_capture');enum(config.security.ipc,'same-user','security.ipc')
print('Documentation fixture contract passed: tagged widths/keys, ordered output selectors, field whitelist, references and enums.')
print('Not a hardened helper/parser, complete production schema, regex compiler, or hardware validator.')
