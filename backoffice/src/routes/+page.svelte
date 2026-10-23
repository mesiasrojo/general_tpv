<script lang="ts">
  import { onMount } from 'svelte';

  interface SyncEvent {
    event_id: string;
    store_id: string;
    device_id: string;
    aggregate_type: string;
    event_type: string;
    occurred_at: string;
    payload: Record<string, unknown>;
  }

  interface StoredEvent {
    seq: number;
    event: SyncEvent;
    received_at: string;
  }

  interface Health {
    status: string;
    events: number;
    cursor: number;
  }

  interface DeviceRecord {
    device_id: string;
    tenant_id: string;
    store_id: string;
    name: string;
    status: 'active' | 'revoked';
    enrolled_at: string;
    last_seen_at: string | null;
    revoked_at: string | null;
  }

  interface MasterDataChange {
    kind: string;
    entity_id: string;
    version: number;
    device_id: string;
    occurred_at: string;
    payload: Record<string, unknown>;
    deleted: boolean;
  }

  interface MasterDataConflict {
    id: string;
    tenant_id: string;
    store_id: string;
    kind: string;
    entity_id: string;
    local_version: number;
    incoming: MasterDataChange;
    reason: string;
    outcome: string;
    detected_at: string;
  }

  const base = (import.meta.env.VITE_CENTRAL_URL ?? 'http://localhost:8080').replace(/\/+$/, '');
  const token = import.meta.env.VITE_CENTRAL_TOKEN ?? '';

  let health = $state<Health | null>(null);
  let events = $state<StoredEvent[]>([]);
  let devices = $state<DeviceRecord[]>([]);
  let conflicts = $state<MasterDataConflict[]>([]);
  let error = $state('');
  let actionError = $state('');
  let loading = $state(false);
  let busyDeviceId = $state('');
  let lastUpdated = $state('');

  let sales = $derived(events.filter((item) => item.event.aggregate_type === 'sale'));
  let invoices = $derived(events.filter((item) => item.event.aggregate_type === 'invoice'));
  let lastSale = $derived(sales.at(-1) ?? null);
  let activeDevices = $derived(devices.filter((device) => device.status === 'active').length);

  onMount(() => {
    void load();
  });

  async function load() {
    loading = true;
    error = '';
    actionError = '';
    const headers: Record<string, string> = {};
    if (token) headers.Authorization = `Bearer ${token}`;
    try {
      const [healthResponse, eventsResponse, devicesResponse, conflictsResponse] = await Promise.all([
        fetch(`${base}/health`),
        fetch(`${base}/api/v1/events`, { headers }),
        fetch(`${base}/api/v1/devices`, { headers }),
        fetch(`${base}/api/v1/master-data/conflicts`, { headers })
      ]);
      if (!healthResponse.ok) throw new Error(`health HTTP ${healthResponse.status}`);
      if (!eventsResponse.ok) throw new Error(`events HTTP ${eventsResponse.status}`);
      if (!devicesResponse.ok) throw new Error(`devices HTTP ${devicesResponse.status}`);
      if (!conflictsResponse.ok) throw new Error(`conflicts HTTP ${conflictsResponse.status}`);
      health = (await healthResponse.json()) as Health;
      events = (await eventsResponse.json()) as StoredEvent[];
      devices = (await devicesResponse.json()) as DeviceRecord[];
      conflicts = (await conflictsResponse.json()) as MasterDataConflict[];
      lastUpdated = new Date().toLocaleString('es-ES');
    } catch (cause) {
      error = `No se pudo leer la central (${base}): ${String(cause)}`;
    } finally {
      loading = false;
    }
  }

  async function revokeDevice(device: DeviceRecord) {
    if (device.status === 'revoked' || busyDeviceId) return;
    if (!window.confirm(`¿Revocar el dispositivo "${device.name}"? Dejará de poder sincronizar.`)) {
      return;
    }
    busyDeviceId = device.device_id;
    actionError = '';
    const headers: Record<string, string> = {};
    if (token) headers.Authorization = `Bearer ${token}`;
    try {
      const response = await fetch(`${base}/api/v1/devices/${device.device_id}/revoke`, {
        method: 'POST',
        headers
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const updated = (await response.json()) as DeviceRecord;
      devices = devices.map((item) => (item.device_id === updated.device_id ? updated : item));
    } catch (cause) {
      actionError = `No se pudo revocar ${device.name}: ${String(cause)}`;
    } finally {
      busyDeviceId = '';
    }
  }

  function formatDate(value: string | null): string {
    return value ? new Date(value).toLocaleString('es-ES') : '—';
  }

  function shortId(value: string): string {
    return value.length > 12 ? `${value.slice(0, 8)}…` : value;
  }

  function statusLabel(status: DeviceRecord['status']): string {
    return status === 'active' ? 'Activo' : 'Revocado';
  }

  function conflictPayload(conflict: MasterDataConflict): string {
    const value = JSON.stringify(conflict.incoming.payload);
    return value.length > 90 ? `${value.slice(0, 90)}…` : value;
  }

  function money(value: unknown): string {
    return typeof value === 'number' ? `${(value / 100).toFixed(2)} €` : '—';
  }

  function payloadValue(event: SyncEvent, key: string): string {
    const value = event.payload[key];
    if (typeof value === 'string' || typeof value === 'number') return String(value);
    return '—';
  }
</script>

<svelte:head>
  <title>General TPV · Central</title>
</svelte:head>

<main class="shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">GENERAL TPV · CENTRAL</p>
      <h1>Consolidación multi-tienda</h1>
      <p class="muted">Eventos recibidos desde las cajas. Solo lectura, sin reescribir ventas.</p>
    </div>
    <div class="actions">
      <span class="status" class:ok={!error}>{error ? 'SIN CONEXIÓN' : 'CONECTADO'}</span>
      <button type="button" onclick={() => void load()} disabled={loading}>
        {loading ? 'Actualizando…' : 'Actualizar'}
      </button>
    </div>
  </header>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {/if}
  {#if actionError}
    <p class="error" role="alert">{actionError}</p>
  {/if}

  <section class="cards">
    <article>
      <p class="eyebrow">EVENTOS</p>
      <strong>{health?.events ?? events.length}</strong>
      <span>cursor {health?.cursor ?? '—'}</span>
    </article>
    <article>
      <p class="eyebrow">VENTAS</p>
      <strong>{sales.length}</strong>
      <span>última {lastSale ? formatDate(lastSale.event.occurred_at) : '—'}</span>
    </article>
    <article>
      <p class="eyebrow">FACTURAS</p>
      <strong>{invoices.length}</strong>
      <span>eventos emitidos</span>
    </article>
    <article>
      <p class="eyebrow">DISPOSITIVOS</p>
      <strong>{activeDevices}</strong>
      <span>{devices.length} registrados</span>
    </article>
    <article>
      <p class="eyebrow">CONFLICTOS</p>
      <strong>{conflicts.length}</strong>
      <span>datos maestros</span>
    </article>
    <article>
      <p class="eyebrow">ÚLTIMA LECTURA</p>
      <strong class="small">{lastUpdated || '—'}</strong>
      <span>{base}</span>
    </article>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <p class="eyebrow">TERMINALES</p>
        <h2>Dispositivos registrados</h2>
      </div>
      <span class="muted small">{activeDevices} activos · {devices.length - activeDevices} revocados</span>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Nombre</th>
            <th>Tienda</th>
            <th>Dispositivo</th>
            <th>Estado</th>
            <th>Alta</th>
            <th>Último contacto</th>
            <th>Acción</th>
          </tr>
        </thead>
        <tbody>
          {#each devices as device (device.device_id)}
            <tr>
              <td class="strong">{device.name}</td>
              <td class="mono">{shortId(device.store_id)}</td>
              <td class="mono">{shortId(device.device_id)}</td>
              <td>
                <span class="pill" class:revoked={device.status === 'revoked'}>
                  {statusLabel(device.status)}
                </span>
              </td>
              <td>{formatDate(device.enrolled_at)}</td>
              <td>{formatDate(device.last_seen_at)}</td>
              <td>
                <button
                  class="danger"
                  type="button"
                  disabled={device.status === 'revoked' || busyDeviceId === device.device_id}
                  onclick={() => void revokeDevice(device)}
                >
                  {busyDeviceId === device.device_id ? 'Revocando…' : 'Revocar'}
                </button>
              </td>
            </tr>
          {:else}
            <tr>
              <td colspan="7" class="empty">Todavía no hay dispositivos registrados.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <p class="eyebrow">DATOS MAESTROS</p>
        <h2>Conflictos detectados</h2>
      </div>
      <span class="muted small">Misma versión con contenido divergente</span>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Detectado</th>
            <th>Tipo</th>
            <th>Entidad</th>
            <th>Tienda</th>
            <th>Versión</th>
            <th>Resultado</th>
            <th>Payload entrante</th>
          </tr>
        </thead>
        <tbody>
          {#each conflicts as conflict (conflict.id)}
            <tr>
              <td>{formatDate(conflict.detected_at)}</td>
              <td><span class="pill">{conflict.kind}</span></td>
              <td class="mono">{shortId(conflict.entity_id)}</td>
              <td class="mono">{shortId(conflict.store_id)}</td>
              <td class="mono">{conflict.local_version} → {conflict.incoming.version}</td>
              <td>{conflict.outcome === 'accept_incoming' ? 'Entrante' : 'Local'}</td>
              <td class="mono small" title={conflictPayload(conflict)}>{conflictPayload(conflict)}</td>
            </tr>
          {:else}
            <tr>
              <td colspan="7" class="empty">Sin conflictos de datos maestros.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <p class="eyebrow">VENTAS</p>
        <h2>Eventos de venta</h2>
      </div>
      <span class="muted small">Ordenados por secuencia de la central</span>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Seq.</th>
            <th>Evento</th>
            <th>Tienda</th>
            <th>Dispositivo</th>
            <th>Ocurrido</th>
            <th>Total</th>
          </tr>
        </thead>
        <tbody>
          {#each sales as item (item.event.event_id)}
            <tr>
              <td class="mono">{item.seq}</td>
              <td><span class="pill">{item.event.event_type}</span></td>
              <td class="mono">{item.event.store_id.slice(0, 8)}…</td>
              <td class="mono">{item.event.device_id.slice(0, 8)}…</td>
              <td>{formatDate(item.event.occurred_at)}</td>
              <td class="num strong">{money(item.event.payload.total_cents)}</td>
            </tr>
          {:else}
            <tr>
              <td colspan="6" class="empty">Todavía no hay ventas sincronizadas.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <section class="panel">
    <div class="panel-heading">
      <div>
        <p class="eyebrow">TRAZABILIDAD</p>
        <h2>Últimos eventos</h2>
      </div>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Seq.</th>
            <th>Tipo</th>
            <th>Agregado</th>
            <th>Recibido</th>
            <th>Hash</th>
          </tr>
        </thead>
        <tbody>
          {#each events.slice(-20).reverse() as item (item.event.event_id)}
            <tr>
              <td class="mono">{item.seq}</td>
              <td>{item.event.event_type}</td>
              <td class="mono">{item.event.aggregate_type} · {payloadValue(item.event, 'id').slice(0, 8)}</td>
              <td>{formatDate(item.received_at)}</td>
              <td class="mono small">{item.event.event_id.slice(0, 12)}…</td>
            </tr>
          {:else}
            <tr>
              <td colspan="5" class="empty">Sin eventos.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #081114;
    color: #edf7f5;
    font-family: "Segoe UI", system-ui, sans-serif;
  }

  .shell {
    max-width: 1240px;
    margin: 0 auto;
    padding: 32px 24px 64px;
  }

  .topbar {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 24px;
  }

  h1 {
    margin: 4px 0 6px;
    font-size: clamp(1.5rem, 3vw, 2.3rem);
  }

  h2 {
    margin: 4px 0 0;
    font-size: 1.15rem;
  }

  .eyebrow {
    margin: 0;
    color: #78e0b0;
    font-size: 0.7rem;
    font-weight: 800;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  .muted {
    color: #9fb8b5;
  }

  .small {
    font-size: 0.82rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .status {
    border: 1px solid #6e4040;
    color: #ffb0b0;
    border-radius: 999px;
    padding: 6px 10px;
    font-size: 0.72rem;
    font-weight: 800;
  }

  .status.ok {
    border-color: #2b6650;
    color: #9bf0bd;
  }

  button {
    background: #78e0b0;
    color: #07120c;
    border: 0;
    border-radius: 10px;
    padding: 10px 14px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  button.danger {
    background: #7a3f3f;
    color: #ffecec;
    padding: 7px 10px;
    font-size: 0.8rem;
  }

  .error {
    border: 1px solid #7a3f3f;
    background: #2a1515;
    color: #ffb0b0;
    padding: 12px 14px;
    border-radius: 10px;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 14px;
    margin: 20px 0;
  }

  .cards article {
    background: #111f24;
    border: 1px solid #284047;
    border-radius: 16px;
    padding: 16px;
    display: grid;
    gap: 4px;
  }

  .cards strong {
    font-size: 2rem;
    font-variant-numeric: tabular-nums;
  }

  .cards strong.small {
    font-size: 0.95rem;
  }

  .cards span {
    color: #9fb8b5;
    font-size: 0.8rem;
  }

  .panel {
    background: #111f24;
    border: 1px solid #284047;
    border-radius: 18px;
    padding: 18px;
    margin-top: 18px;
  }

  .panel-heading {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-end;
    margin-bottom: 12px;
  }

  .table-wrap {
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }

  th,
  td {
    text-align: left;
    padding: 10px 8px;
    border-bottom: 1px solid #284047;
    white-space: nowrap;
  }

  th {
    color: #9fb8b5;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .strong {
    font-weight: 700;
  }

  .mono {
    font-family: "Cascadia Code", ui-monospace, monospace;
    font-variant-numeric: tabular-nums;
  }

  .pill {
    border: 1px solid #2b6650;
    background: #10251e;
    color: #9bf0bd;
    border-radius: 999px;
    padding: 3px 8px;
    font-size: 0.75rem;
  }

  .pill.revoked {
    border-color: #6e4040;
    background: #2a1515;
    color: #ffb0b0;
  }

  .empty {
    color: #9fb8b5;
    text-align: center;
    padding: 28px;
  }

  @media (max-width: 900px) {
    .cards {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .topbar {
      flex-direction: column;
    }
  }
</style>
