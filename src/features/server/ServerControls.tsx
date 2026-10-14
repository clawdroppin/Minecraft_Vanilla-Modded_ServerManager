import { AlarmClock, Copy, Download, FolderOpen, Layers, MoreHorizontal, Play, RefreshCw, RotateCw, Skull, Square, TimerOff, Trash2, Upload } from 'lucide-react';
import { Button, IconButton } from '@/components/ui/Button';
import { Menu } from '@/components/ui/Menu';
import { useServerActions } from '@/hooks/useServerActions';
import { api } from '@/lib/ipc';
import { reveal } from '@/lib/shell';
import { useApp, useInstance, useStatus } from '@/store/app';
import { errMsg } from '@/lib/ipc';

export function ServerControls({ id, size = 'md', compact }: { id: string; size?: 'sm' | 'md' | 'lg'; compact?: boolean }) {
  const st = useStatus(id);
  const inst = useInstance(id);
  const { run, pending } = useServerActions(id);
  const state = st?.state ?? 'stopped';
  const setDialog = useApp((s) => s.setDialog);
  const toast = useApp((s) => s.toast);
  if (!inst) return null;

  const live = state === 'running' || state === 'starting';
  const transitioning = state === 'stopping' || state === 'restarting' || state === 'installing' || state === 'updating';

  return (
    <div className="flex items-center gap-1.5">
      {state === 'notInstalled' ? (
        <Button variant="primary" size={size} icon={<Download className="size-4" />} loading={pending === 'install'} onClick={() => run('install')}>
          Install server
        </Button>
      ) : live || state === 'stopping' || state === 'restarting' ? (
        <>
          <Button variant="secondary" size={size} icon={<Square className="size-3.5 fill-current" />} loading={pending === 'stop' || state === 'stopping'} disabled={transitioning} onClick={() => run('stop')}>
            {compact ? null : state === 'stopping' ? 'Stopping…' : 'Stop'}
          </Button>
          <Button variant="secondary" size={size} icon={<RotateCw className="size-3.5" />} loading={pending === 'restart' || state === 'restarting'} disabled={transitioning} onClick={() => run('restart')}>
            {compact ? null : 'Restart'}
          </Button>
        </>
      ) : (
        <Button variant="primary" size={size} icon={<Play className="size-3.5 fill-current" />} loading={pending === 'start'} disabled={transitioning} onClick={() => run('start')}>
          {compact ? null : transitioning ? (state === 'installing' ? 'Installing…' : 'Updating…') : 'Start'}
        </Button>
      )}
      {!compact && (
        <Menu
          width={240}
          trigger={() => <IconButton label="More actions" className="size-8 border border-line-strong bg-panel-2"><MoreHorizontal /></IconButton>}
          items={[
            { label: 'Restart in 5 minutes (warn players)', icon: <AlarmClock />, disabled: state !== 'running', onSelect: () => api.startCountdown(id, 'restart', 300).catch((e) => toast('error', errMsg(e), id)) },
            { label: 'Stop in 1 minute (warn players)', icon: <AlarmClock />, disabled: state !== 'running', onSelect: () => api.startCountdown(id, 'stop', 60).catch((e) => toast('error', errMsg(e), id)) },
            { label: 'Cancel countdown', icon: <TimerOff />, disabled: !st?.countdown, onSelect: () => api.cancelCountdown(id) },
            { label: 'Force stop (no save)', icon: <Skull />, danger: true, disabled: !live && state !== 'stopping', onSelect: () => run('forceStop') },
            { separator: true, label: '' },
            { label: 'Check for updates', icon: <RefreshCw />, disabled: state === 'notInstalled', onSelect: () => api.checkUpdate(id).then((u) => toast(u ? 'info' : 'success', u ? u.label : 'Up to date', id)).catch((e) => toast('error', errMsg(e), id)) },
            { label: 'Change version / software…', icon: <Layers />, onSelect: () => useApp.getState().openServer(id, 'software') },
            { label: 'Reinstall server files', icon: <Download />, disabled: transitioning, onSelect: () => run('install') },
            { separator: true, label: '' },
            { label: 'Open server folder', icon: <FolderOpen />, onSelect: () => reveal(inst.installDir) },
            { label: 'Export server…', icon: <Upload />, onSelect: () => setDialog({ kind: 'export', id }) },
            { label: 'Duplicate server…', icon: <Copy />, onSelect: () => setDialog({ kind: 'clone', id }) },
            { separator: true, label: '' },
            { label: 'Delete server…', icon: <Trash2 />, danger: true, onSelect: () => setDialog({ kind: 'delete', id }) },
          ]}
        />
      )}
    </div>
  );
}
