import json,os,signal,sys,time
folder,tag,policy=sys.argv[1:]
def event(kind):
    path=os.path.join(folder,str(os.getpid())+'-'+kind+'.json')
    tmp=path+'.next'
    with open(tmp,'w') as f:
        json.dump(dict(pid=os.getpid(),tag=tag,kind=kind,wayland=os.getenv('WAYLAND_DISPLAY'),ipc=os.getenv('FLUXWM_SOCKET'),display=os.getenv('DISPLAY'),socket=os.getenv('WAYLAND_SOCKET'),blocked=sorted(int(s) for s in signal.pthread_sigmask(signal.SIG_BLOCK,[]))),f)
    os.replace(tmp,path)
def terminate(*_):
    event('term')
    if policy != 'ignore': sys.exit(0)
signal.signal(signal.SIGTERM,terminate)
event('ready')
if policy != 'fast': time.sleep(10)
