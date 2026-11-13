# Trusted collector: task cgroup is frozen; this container shares only its PID namespace.
import os, stat, sys, tarfile
max_nodes, max_bytes = map(int, sys.argv[1:])
root = '/proc/1/root/work'
nodes, total = 0, 0
with tarfile.open(fileobj=sys.stdout.buffer, mode='w|', format=tarfile.PAX_FORMAT) as archive:
    pending = [(root, '')]
    while pending:
        directory, prefix = pending.pop()
        for name in sorted(os.listdir(directory)):
            if name in ('', '.', '..') or '/' in name or '\0' in name:
                raise ValueError('invalid task name')
            name.encode('utf-8', errors='strict')
            path = os.path.join(directory, name)
            relative = prefix + name
            info = os.lstat(path)
            nodes += 1
            if nodes > max_nodes: raise ValueError('collection node budget')
            member = tarfile.TarInfo(relative)
            if stat.S_ISLNK(info.st_mode):
                member.type = tarfile.SYMTYPE
                member.linkname = os.readlink(path)
                member.linkname.encode('utf-8', errors='strict')
                archive.addfile(member)
            elif stat.S_ISDIR(info.st_mode):
                member.type = tarfile.DIRTYPE
                member.mode = 0o755
                archive.addfile(member)
                pending.append((path, relative + '/'))
            elif stat.S_ISREG(info.st_mode):
                total += info.st_size
                if info.st_size < 0 or total > max_bytes: raise ValueError('collection byte budget')
                member.size = info.st_size
                member.mode = 0o755 if info.st_mode & 0o111 else 0o644
                descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(descriptor, 'rb') as source:
                    if not stat.S_ISREG(os.fstat(source.fileno()).st_mode): raise ValueError('not a regular file')
                    archive.addfile(member, source)
            else:
                raise ValueError('unsupported task filesystem object')
