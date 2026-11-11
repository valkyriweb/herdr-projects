# Reproduce the existing warm-start shell contract with stand-in files, without compiling anything.
from pathlib import Path
import os, subprocess, tempfile
base = Path(tempfile.mkdtemp(prefix='sw-warm-repro-')).resolve()
repo = base / 'repo'; (repo / 'src').mkdir(parents=True)
env = dict(os.environ, GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1', XDG_CACHE_HOME=str(base/'cache'))
for k in ['CARGO_TARGET_DIR', 'HERDR_PROJECTS_NO_WARM']: env.pop(k, None)
def cmd(args, cwd=repo):
    p = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True)
    if p.returncode: raise RuntimeError((args, p.stdout, p.stderr))
    return p.stdout.strip()
def git(*args): return cmd(['git','-c','user.name=t','-c','user.email=t@t',*args])
def commit(text):
    (repo/'src/a.rs').write_text(text); git('add','-A'); git('commit','-qm',text); return git('rev-parse','HEAD')
def wt(name, ref): git('worktree','add','-qb','work/'+name,str(base/name),ref)
script = Path(__file__).parent/'scripts/warm-start.sh'
def setup(name):
    source = f"set -ex; . '{script}'; ws_seed; mkdir -p target/quick; printf 'built in {name}' > target/quick/herdr-projects; ws_keep"
    p = subprocess.run(['/bin/sh','-c',source],cwd=base/name,env=env,text=True,capture_output=True)
    (base/(name+'.log')).write_text(p.stdout+'\n'+p.stderr)
    print(name, p.returncode, p.stdout.strip())
    for root in (base/'cache/herdr-projects/warm').glob('*'):
        cur=(root/'current').read_text().strip(); print('kept',root, (root/cur/'commit').read_text().strip())
git('init','-qb','main'); (repo/'.gitignore').write_text('/target/\n'); first=commit('a'); second=commit('a2')
wt('new','main'); setup('new'); wt('old',first); setup('old')
print('expected',second,'fixture',base)
