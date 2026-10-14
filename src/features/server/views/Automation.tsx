import { AlarmClock, Archive, CalendarClock, HeartPulse, Plus, RefreshCw, Trash2, X, Zap } from 'lucide-react';
import { cn } from '@/lib/cn';
import { until } from '@/lib/format';
import type { ScheduleEntry } from '@/lib/types';
import { useStatus } from '@/store/app';
import { Button, IconButton } from '@/components/ui/Button';
import { FieldRow, Input, NumberInput, Select, Toggle } from '@/components/ui/Field';
import { Card, PageHeader, Section } from '@/components/ui/Surface';
import { useDraft } from '../draft';

const DAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'];
const ACTIONS = [
  { value: 'restart', label: 'Restart (with warnings)' },
  { value: 'backup', label: 'Back up' },
  { value: 'stop', label: 'Stop (with warnings)' },
  { value: 'start', label: 'Start' },
  { value: 'say', label: 'Broadcast a message' },
  { value: 'command', label: 'Run a command' },
];

function ScheduleRow({ e, onChange, onRemove }: { e: ScheduleEntry; onChange: (e: ScheduleEntry) => void; onRemove: () => void }) {
  return (
    <div className={cn('flex items-center gap-3 py-3', !e.enabled && 'opacity-55')}>
      <Toggle size="sm" checked={e.enabled} onChange={(v) => onChange({ ...e, enabled: v })} />
      <Select value={e.action} onChange={(x) => onChange({ ...e, action: x.target.value })} className="w-52">
        {ACTIONS.map((a) => (
          <option key={a.value} value={a.value}>
            {a.label}
          </option>
        ))}
      </Select>
      <span className="text-[12px] text-fg-3">at</span>
      <Input type="time" value={e.time} onChange={(x) => onChange({ ...e, time: x.target.value })} className="w-28 font-mono [color-scheme:dark]" />
      <div className="flex gap-1">
        {DAYS.map((d, i) => {
          const on = e.days.length === 0 || e.days.includes(i);
          return (
            <button
              key={d}
              onClick={() => {
                const cur = e.days.length === 0 ? [0, 1, 2, 3, 4, 5, 6] : e.days;
                const next = cur.includes(i) ? cur.filter((x) => x !== i) : [...cur, i].sort();
                if (next.length === 0) return; // at least one day
                onChange({ ...e, days: next.length === 7 ? [] : next });
              }}
              className={cn('h-7 w-8 rounded-md border text-[11px] font-medium', on ? 'border-accent/50 bg-accent/12 text-accent' : 'border-line-strong text-fg-4 hover:text-fg-2')}
            >
              {d}
            </button>
          );
        })}
      </div>
      {(e.action === 'command' || e.action === 'say') && (
        <Input mono value={e.command} onChange={(x) => onChange({ ...e, command: x.target.value })} placeholder={e.action === 'say' ? 'Vote for us at example.com!' : 'weather clear'} className="w-56" />
      )}
      <span className="ml-1 text-[11px] text-fg-4">{e.days.length === 0 ? 'every day' : ''}</span>
      <IconButton label="Remove" className="ml-auto hover:text-err" onClick={onRemove}>
        <Trash2 />
      </IconButton>
    </div>
  );
}

export function Automation({ id }: { id: string }) {
  const { draft, patch } = useDraft(id);
  const st = useStatus(id);
  if (!draft) return null;
  const a = draft.automation;
  const set = <K extends keyof typeof a>(k: K, v: (typeof a)[K]) => patch((d) => void (d.automation[k] = v));

  return (
    <div className="mx-auto max-w-[1000px] space-y-5 p-6 pb-24">
      <PageHeader title="Automation" description="Keep the server healthy without babysitting it – crash recovery, schedules, backups and updates. The manager must be running for automation to work." />

      <Section title="Uptime & recovery" icon={<HeartPulse />} bodyClassName="divide-y divide-line">
        <FieldRow label="Start with the manager" description="Start this server automatically when the manager opens.">
          <Toggle checked={a.autoStart} onChange={(v) => set('autoStart', v)} />
        </FieldRow>
        <FieldRow label="Restart after a crash" description="Bring the server back automatically if it exits unexpectedly.">
          <Toggle checked={a.autoRestart} onChange={(v) => set('autoRestart', v)} />
        </FieldRow>
        <FieldRow label="Restart delay" description="Wait before restarting after a crash.">
          <NumberInput value={a.restartDelaySecs} min={0} max={600} step={5} suffix="s" onChange={(v) => set('restartDelaySecs', v)} />
        </FieldRow>
        <FieldRow label="Crash-loop guard" description="Pause auto-restart after this many crashes within 15 minutes, so a broken config doesn’t restart forever. 0 = never pause.">
          <NumberInput value={a.crashLimit} min={0} max={50} suffix="crashes" onChange={(v) => set('crashLimit', v)} />
        </FieldRow>
        <FieldRow label="Graceful stop timeout" description="How long to wait for the server to save and exit before it is force-closed.">
          <NumberInput value={a.stopTimeoutSecs} min={5} max={900} step={5} suffix="s" onChange={(v) => set('stopTimeoutSecs', v)} />
        </FieldRow>
        <FieldRow label="Restart above memory" description="Restart when the server stays above this much RAM for 30 seconds – a safety net for leaks. 0 = off.">
          <NumberInput value={a.memoryLimitMb} min={0} max={262144} step={512} suffix="MB" onChange={(v) => set('memoryLimitMb', v)} />
        </FieldRow>
        <FieldRow label="Restart after uptime" description="Restart once the server has been up this long. 0 = off. A daily scheduled restart is usually nicer for players.">
          <NumberInput value={a.maxUptimeHours} min={0} max={720} suffix="h" onChange={(v) => set('maxUptimeHours', v)} />
        </FieldRow>
        <FieldRow label="Restart when empty" description="Restart after nobody has been online for this long (only after someone played) – frees memory leaked by mods. 0 = off.">
          <NumberInput value={a.emptyRestartMins} min={0} max={1440} step={15} suffix="min" onChange={(v) => set('emptyRestartMins', v)} />
        </FieldRow>
      </Section>

      <Section title="Restart warnings" icon={<AlarmClock />} description="Scheduled and automatic restarts/stops count down in chat first, so nobody loses progress. Skipped when nobody is online." bodyClassName="divide-y divide-line">
        <FieldRow label="Warn players at" description="Seconds before the restart.">
          <div className="flex flex-wrap items-center justify-end gap-1.5">
            {[...a.restartWarnings].sort((x, y) => y - x).map((w) => (
              <span key={w} className="flex h-7 items-center gap-1 rounded-md border border-line-strong bg-panel-2 pr-1 pl-2 font-mono text-[11.5px] text-fg-2">
                {w >= 60 && w % 60 === 0 ? `${w / 60}m` : `${w}s`}
                <button className="text-fg-4 hover:text-err" onClick={() => set('restartWarnings', a.restartWarnings.filter((x) => x !== w))}>
                  <X className="size-3" />
                </button>
              </span>
            ))}
            <Select
              value=""
              onChange={(e) => {
                const v = Number(e.target.value);
                if (v && !a.restartWarnings.includes(v)) set('restartWarnings', [...a.restartWarnings, v]);
              }}
              className="w-24"
            >
              <option value="">+ add</option>
              {[900, 600, 300, 120, 60, 30, 10, 5].map((v) => (
                <option key={v} value={v}>
                  {v >= 60 ? `${v / 60} min` : `${v} s`}
                </option>
              ))}
            </Select>
          </div>
        </FieldRow>
        <FieldRow label="Message" description="{time} becomes “5 minutes”, “10 seconds”…">
          <Input value={a.warningMessage} onChange={(e) => set('warningMessage', e.target.value)} />
        </FieldRow>
      </Section>

      <Section
        title="Schedules"
        icon={<CalendarClock />}
        description={st?.nextSchedule ? `Next: ${st.nextSchedule.action} ${until(st.nextSchedule.at)}` : 'Run actions at set times (local time).'}
        actions={
          <Button size="sm" icon={<Plus className="size-3.5" />} onClick={() => patch((d) => void d.automation.schedules.push({ enabled: true, time: '05:00', days: [], action: 'restart', command: '' }))}>
            Add schedule
          </Button>
        }
        bodyClassName="divide-y divide-line"
      >
        {a.schedules.length === 0 ? (
          <div className="py-6 text-center text-[12.5px] text-fg-4">
            No schedules. A daily restart at a quiet hour keeps memory fresh and picks up updates.
            <div className="mt-3 flex justify-center gap-2">
              <Button size="sm" onClick={() => patch((d) => void d.automation.schedules.push({ enabled: true, time: '05:00', days: [], action: 'restart', command: '' }))}>
                Daily restart at 05:00
              </Button>
              <Button size="sm" onClick={() => patch((d) => void d.automation.schedules.push({ enabled: true, time: '12:00', days: [], action: 'backup', command: '' }))}>
                Daily backup at noon
              </Button>
            </div>
          </div>
        ) : (
          a.schedules.map((e, i) => (
            <ScheduleRow
              key={i}
              e={e}
              onChange={(n) => patch((d) => void (d.automation.schedules[i] = n))}
              onRemove={() => patch((d) => void d.automation.schedules.splice(i, 1))}
            />
          ))
        )}
      </Section>

      <Section title="Backups" icon={<Archive />} bodyClassName="divide-y divide-line">
        <FieldRow label="Back up while running" description="Snapshot the world every N minutes while the server is online (saving is paused with save-off for a consistent copy). 0 = off.">
          <NumberInput value={a.backupIntervalMins} min={0} max={1440} step={15} suffix="min" onChange={(v) => set('backupIntervalMins', v)} />
        </FieldRow>
        <FieldRow label="Skip when nobody played" description="Don’t make interval backups of a world nobody touched since the last one.">
          <Toggle checked={a.backupOnlyIfPlayers} onChange={(v) => set('backupOnlyIfPlayers', v)} />
        </FieldRow>
        <FieldRow label="Include mods / plugins" description="Also zip the mods or plugins folder (bigger backups).">
          <Toggle checked={a.backupMods} onChange={(v) => set('backupMods', v)} />
        </FieldRow>
        <FieldRow label="Keep automatic backups" description="Older automatic backups are deleted beyond this count. Manual backups are always kept. 0 = keep all.">
          <NumberInput value={a.backupKeep} min={0} max={1000} suffix="backups" onChange={(v) => set('backupKeep', v)} />
        </FieldRow>
        <FieldRow label="Back up after stopping" description="Take a snapshot each time the server shuts down cleanly.">
          <Toggle checked={a.backupOnStop} onChange={(v) => set('backupOnStop', v)} />
        </FieldRow>
        <FieldRow label="Back up before version changes" description="Snapshot before switching Minecraft version, loader or build.">
          <Toggle checked={a.backupBeforeUpdate} onChange={(v) => set('backupBeforeUpdate', v)} />
        </FieldRow>
      </Section>

      <Section title="Updates" icon={<RefreshCw />} description="New Minecraft releases are announced app-wide. Here: newer builds of what you run (Paper builds, loader and Forge patches)." bodyClassName="divide-y divide-line">
        <FieldRow label="Check for new builds" description="Look for a newer build / loader of the installed Minecraft version every N minutes. 0 = off.">
          <NumberInput value={a.updateCheckMins} min={0} max={1440} step={15} suffix="min" onChange={(v) => set('updateCheckMins', v)} />
        </FieldRow>
        <FieldRow
          label={<span className="flex items-center gap-1.5"><Zap className="size-3.5 text-accent" /> Install new builds automatically</span>}
          description="Stays on the same Minecraft version – never upgrades the world. Players get the restart countdown first."
        >
          <Toggle checked={a.autoUpdateBuilds} onChange={(v) => set('autoUpdateBuilds', v)} />
        </FieldRow>
      </Section>
      <Card className="p-4 text-[12px] text-fg-3">
        Tip: keep the manager running (it can sit minimised) – crash recovery, schedules and update checks run inside it. Servers themselves keep running if you close the manager, and are picked up again when it reopens.
      </Card>
    </div>
  );
}
