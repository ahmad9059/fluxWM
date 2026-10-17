-- Host-side probe, not FluxWM configuration.
-- Tests whether ordinary Lua instruction-hook errors can be caught by pcall.
local trips, caught = 0, 0
debug.sethook(function()
  trips = trips + 1
  error('instruction budget exceeded')
end, '', 1000)
for i = 1, 5 do
  local ok, err = pcall(function() while true do end end)
  if not ok and tostring(err):find('instruction budget exceeded', 1, true) then
    caught = caught + 1
  end
end
debug.sethook()
assert(trips == 5 and caught == 5)
print('instruction-hook errors: trips=' .. trips .. ', caught=' .. caught)
print('Conclusion: hook errors alone do not force the outer config to terminate.')
