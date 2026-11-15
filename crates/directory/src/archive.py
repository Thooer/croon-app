# Parse a bounded Docker archive without extracting through archive paths.
import os, sys, tarfile
archive, destination, max_nodes, max_bytes = sys.argv[1:]
max_nodes, max_bytes = int(max_nodes), int(max_bytes)
seen, nodes, total = set(), 0, 0
with tarfile.open(archive, mode='r|') as tar:
    for member in tar:
        name = member.name
        parts = name.split('/')
        if parts[0] == '.': parts = parts[1:]
        if parts and parts[-1] == '': parts = parts[:-1]
        if not parts:
            if not member.isdir(): raise ValueError('non-directory archive root')
            continue
        if any(p in ('', '.', '..') or '\0' in p for p in parts):
            raise ValueError('unsafe archive path')
        name = '/'.join(parts)
        if name in seen: raise ValueError('duplicate archive member')
        seen.add(name)
        nodes += 1
        if nodes > max_nodes: raise ValueError('archive node budget')
        path = os.path.join(destination, *parts)
        parent = destination
        for part in parts[:-1]:
            parent = os.path.join(parent, part)
            if not os.path.isdir(parent) or os.path.islink(parent):
                raise ValueError('archive parent is not a real directory')
        if member.isdir():
            os.mkdir(path, 0o700)
        elif member.isfile():
            if member.size < 0: raise ValueError('negative file size')
            total += member.size
            if total > max_bytes: raise ValueError('archive byte budget')
            with tar.extractfile(member) as source, open(path, 'xb') as output:
                remaining = member.size
                while remaining:
                    block = source.read(min(remaining, 65536))
                    if not block: raise ValueError('short archive member')
                    output.write(block)
                    remaining -= len(block)
            os.chmod(path, 0o755 if member.mode & 0o111 else 0o644)
        elif member.issym():
            if '\0' in member.linkname: raise ValueError('NUL in link')
            os.symlink(member.linkname, path)
        else:
            raise ValueError('unsupported archive object, including hardlink')
