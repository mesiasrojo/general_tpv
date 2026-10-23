<script lang="ts">
  import { onMount } from 'svelte';
  import '../../../themes.css';
  import { applyTheme, nextTheme, storedTheme, type Theme } from '$lib/theme';

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

  type UserRole = 'admin' | 'manager' | 'cashier' | 'auditor';

  interface CentralUser {
    id: string;
    tenant_id: string;
    username: string;
    display_name: string;
    role: UserRole;
    store_ids: string[];
    active: boolean;
    version: number;
    created_at: string;
    updated_at: string;
  }

  interface AuthUser {
    id: string | null;
    tenant_id: string | null;
    username: string;
    display_name: string;
    role: UserRole;
  }

  interface AuthStatus {
    has_users: boolean;
    suggested_tenant_id: string | null;
  }

  interface AuthSession {
    token: string;
    expires_at: string;
    user: AuthUser;
  }

  const base = (import.meta.env.VITE_CENTRAL_URL ?? 'http://localhost:8080').replace(/\/+$/, '');
  const configuredToken = import.meta.env.VITE_CENTRAL_TOKEN ?? '';
  const sessionKey = 'general-tpv-backoffice-session';

  let authToken = $state('');
  let authUser = $state<AuthUser | null>(null);
  let authStatus = $state<AuthStatus | null>(null);
  let authChecking = $state(true);
  let authBusy = $state(false);
  let authError = $state('');
  let loginUsername = $state('');
  let loginPassword = $state('');
  let bootstrapTenantId = $state('');
  let bootstrapUsername = $state('');
  let bootstrapDisplayName = $state('Administrador');
  let bootstrapPassword = $state('');

  let health = $state<Health | null>(null);
  let events = $state<StoredEvent[]>([]);
  let devices = $state<DeviceRecord[]>([]);
  let conflicts = $state<MasterDataConflict[]>([]);
  let theme = $state<Theme>('light_theme');
  let centralUsers = $state<CentralUser[]>([]);
  let error = $state('');
  let actionError = $state('');
  let usersError = $state('');
  let loading = $state(false);
  let busyDeviceId = $state('');
  let busyUserId = $state('');
  let userCreating = $state(false);
  let lastUpdated = $state('');

  let newTenantId = $state('');
  let newStoreIds = $state('');
  let newUsername = $state('');
  let newDisplayName = $state('');
  let newUserRole = $state<UserRole>('cashier');
  let newUserPassword = $state('');

  let sales = $derived(events.filter((item) => item.event.aggregate_type === 'sale'));
  let invoices = $derived(events.filter((item) => item.event.aggregate_type === 'invoice'));
  let lastSale = $derived(sales.at(-1) ?? null);
  let activeDevices = $derived(devices.filter((device) => device.status === 'active').length);
  let activeUsers = $derived(centralUsers.filter((user) => user.active).length);
  let canWrite = $derived(authUser?.role === 'admin');

  onMount(() => {
    theme = storedTheme();
    applyTheme(theme);
    void initializeAuth();
  });

  function authHeaders(): Record<string, string> {
    const headers: Record<string, string> = {};
    if (authToken) headers.Authorization = `Bearer ${authToken}`;
    return headers;
  }

  function applySession(session: AuthSession) {
    authToken = session.token;
    authUser = session.user;
    sessionStorage.setItem(sessionKey, session.token);
    authStatus = { has_users: true, suggested_tenant_id: null };
    authError = '';
  }

  function clearSession(message = '') {
    authToken = '';
    authUser = null;
    sessionStorage.removeItem(sessionKey);
    authStatus = { has_users: true, suggested_tenant_id: null };
    authChecking = false;
    authError = message;
  }

  async function initializeAuth() {
    authChecking = true;
    authError = '';
    const stored = sessionStorage.getItem(sessionKey) ?? '';
    authToken = stored || configuredToken;
    if (authToken) {
      try {
        const response = await fetch(`${base}/api/v1/auth/me`, { headers: authHeaders() });
        if (response.ok) {
          authUser = (await response.json()) as AuthUser;
          authChecking = false;
          await load();
          return;
        }
      } catch {
        // Fall through to the login or bootstrap screen.
      }
      authToken = '';
      sessionStorage.removeItem(sessionKey);
    }
    try {
      const response = await fetch(`${base}/api/v1/auth/status`);
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      authStatus = (await response.json()) as AuthStatus;
      if (authStatus.suggested_tenant_id && !bootstrapTenantId) {
        bootstrapTenantId = authStatus.suggested_tenant_id;
      }
    } catch (cause) {
      authError = `No se pudo consultar la central: ${String(cause)}`;
    } finally {
      authChecking = false;
    }
  }

  async function login() {
    authBusy = true;
    authError = '';
    try {
      const response = await fetch(`${base}/api/v1/auth/login`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: loginUsername, password: loginPassword })
      });
      if (!response.ok) throw new Error(await responseMessage(response));
      const session = (await response.json()) as AuthSession;
      applySession(session);
      loginPassword = '';
      await load();
    } catch (cause) {
      authError = `No se pudo iniciar sesión: ${String(cause)}`;
    } finally {
      authBusy = false;
    }
  }

  async function bootstrap() {
    authBusy = true;
    authError = '';
    try {
      const response = await fetch(`${base}/api/v1/auth/bootstrap`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          tenant_id: bootstrapTenantId.trim() || null,
          username: bootstrapUsername,
          display_name: bootstrapDisplayName,
          password: bootstrapPassword
        })
      });
      if (!response.ok) throw new Error(await responseMessage(response));
      const session = (await response.json()) as AuthSession;
      applySession(session);
      bootstrapPassword = '';
      await load();
    } catch (cause) {
      authError = `No se pudo crear el administrador: ${String(cause)}`;
    } finally {
      authBusy = false;
    }
  }

  async function logout() {
    try {
      await fetch(`${base}/api/v1/auth/logout`, {
        method: 'POST',
        headers: authHeaders()
      });
    } catch {
      // Local logout must still work when the central is unavailable.
    }
    clearSession();
    loginPassword = '';
  }

  async function load() {
    if (!authUser) return;
    loading = true;
    error = '';
    actionError = '';
    usersError = '';
    const headers = authHeaders();
    try {
      const [
        healthResponse,
        eventsResponse,
        devicesResponse,
        conflictsResponse,
        usersResponse
      ] = await Promise.all([
        fetch(`${base}/health`),
        fetch(`${base}/api/v1/events`, { headers }),
        fetch(`${base}/api/v1/devices`, { headers }),
        fetch(`${base}/api/v1/master-data/conflicts`, { headers }),
        fetch(`${base}/api/v1/users`, { headers })
      ]);
      if (!healthResponse.ok) throw new Error(`health HTTP ${healthResponse.status}`);
      if ([eventsResponse, devicesResponse, conflictsResponse, usersResponse].some((response) => response.status === 401)) {
        clearSession('La sesión ha caducado. Vuelve a iniciar sesión.');
        return;
      }
      if (!eventsResponse.ok) throw new Error(`events HTTP ${eventsResponse.status}`);
      if (!devicesResponse.ok) throw new Error(`devices HTTP ${devicesResponse.status}`);
      if (!conflictsResponse.ok) throw new Error(`conflicts HTTP ${conflictsResponse.status}`);
      if (!usersResponse.ok) throw new Error(`users HTTP ${usersResponse.status}`);
      health = (await healthResponse.json()) as Health;
      events = (await eventsResponse.json()) as StoredEvent[];
      devices = (await devicesResponse.json()) as DeviceRecord[];
      conflicts = (await conflictsResponse.json()) as MasterDataConflict[];
      centralUsers = (await usersResponse.json()) as CentralUser[];
      const firstDevice = devices[0];
      if (!newTenantId && firstDevice) {
        newTenantId = firstDevice.tenant_id;
      }
      if (!newStoreIds) {
        newStoreIds =
          firstDevice?.store_id ?? events.find((item) => item.event.store_id)?.event.store_id ?? '';
      }
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
    const headers = authHeaders();
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

  function parseStoreIds(value: string): string[] {
    return value
      .split(/[\s,]+/)
      .map((item) => item.trim())
      .filter(Boolean);
  }

  async function responseMessage(response: Response): Promise<string> {
    try {
      const body = (await response.json()) as { error?: string };
      if (body.error) return body.error;
    } catch {
      // Fall back to the HTTP status below.
    }
    return `HTTP ${response.status}`;
  }

  async function createCentralUser() {
    if (userCreating) return;
    usersError = '';
    const storeIds = parseStoreIds(newStoreIds);
    if (!newTenantId.trim() || !newUsername.trim() || !newDisplayName.trim()) {
      usersError = 'Tenant, usuario y nombre visible son obligatorios.';
      return;
    }
    if (storeIds.length === 0) {
      usersError = 'Indica al menos una tienda.';
      return;
    }
    if (newUserPassword.length < 8) {
      usersError = 'La contraseña debe tener al menos 8 caracteres.';
      return;
    }
    userCreating = true;
    const headers = { ...authHeaders(), 'content-type': 'application/json' };
    try {
      const response = await fetch(`${base}/api/v1/users`, {
        method: 'POST',
        headers,
        body: JSON.stringify({
          tenant_id: newTenantId.trim(),
          username: newUsername.trim(),
          display_name: newDisplayName.trim(),
          role: newUserRole,
          store_ids: storeIds,
          password: newUserPassword
        })
      });
      if (!response.ok) throw new Error(await responseMessage(response));
      const user = (await response.json()) as CentralUser;
      centralUsers = [...centralUsers, user];
      newUsername = '';
      newDisplayName = '';
      newUserPassword = '';
    } catch (cause) {
      usersError = `No se pudo crear el usuario: ${String(cause)}`;
    } finally {
      userCreating = false;
    }
  }

  async function toggleCentralUser(user: CentralUser) {
    if (busyUserId) return;
    busyUserId = user.id;
    usersError = '';
    const headers = { ...authHeaders(), 'content-type': 'application/json' };
    try {
      const response = await fetch(`${base}/api/v1/users/${user.id}`, {
        method: 'PUT',
        headers,
        body: JSON.stringify({
          display_name: user.display_name,
          role: user.role,
          store_ids: user.store_ids,
          active: !user.active
        })
      });
      if (!response.ok) throw new Error(await responseMessage(response));
      const updated = (await response.json()) as CentralUser;
      centralUsers = centralUsers.map((item) => (item.id === updated.id ? updated : item));
    } catch (cause) {
      usersError = `No se pudo actualizar ${user.username}: ${String(cause)}`;
    } finally {
      busyUserId = '';
    }
  }

  async function resetCentralUserPassword(user: CentralUser) {
    if (busyUserId) return;
    const password = window.prompt(`Nueva contraseña para ${user.username} (mínimo 8 caracteres)`);
    if (password === null) return;
    if (password.length < 8) {
      usersError = 'La contraseña debe tener al menos 8 caracteres.';
      return;
    }
    busyUserId = user.id;
    usersError = '';
    const headers = { ...authHeaders(), 'content-type': 'application/json' };
    try {
      const response = await fetch(`${base}/api/v1/users/${user.id}/password`, {
        method: 'POST',
        headers,
        body: JSON.stringify({ password })
      });
      if (!response.ok) throw new Error(await responseMessage(response));
      const updated = (await response.json()) as CentralUser;
      centralUsers = centralUsers.map((item) => (item.id === updated.id ? updated : item));
    } catch (cause) {
      usersError = `No se pudo restablecer la contraseña: ${String(cause)}`;
    } finally {
      busyUserId = '';
    }
  }

  function toggleTheme() {
    theme = nextTheme(theme);
    applyTheme(theme);
  }

  function formatDate(value: unknown): string {
    if (!value) return '—';
    if (Array.isArray(value) && value.length >= 6) {
      const [year, ordinal, hour, minute, second, nanosecond, offsetHours = 0, offsetMinutes = 0] =
        value as [number, number, number, number, number, number, number?, number?];
      const utc = Date.UTC(year, 0, 1);
      const local = utc + (ordinal - 1) * 86_400_000;
      const offset = (offsetHours * 60 + offsetMinutes) * 60_000;
      const date = new Date(local + hour * 3_600_000 + minute * 60_000 + second * 1_000 + Math.floor(nanosecond / 1_000_000) - offset);
      return Number.isNaN(date.getTime()) ? '—' : date.toLocaleString('es-ES');
    }
    const date = new Date(String(value));
    return Number.isNaN(date.getTime()) ? '—' : date.toLocaleString('es-ES');
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

  function saleTotal(event: SyncEvent): string {
    if (typeof event.payload.total_cents === 'number') {
      return money(event.payload.total_cents);
    }
    const payments = event.payload.payments;
    if (Array.isArray(payments)) {
      const cents = payments.reduce((sum, payment) => {
        const amount = (payment as { amount?: unknown }).amount;
        return sum + (typeof amount === 'number' ? amount : 0);
      }, 0);
      return money(cents);
    }
    return '—';
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

{#if authChecking}
  <main class="login-shell">
    <section class="panel login-card">
      <p class="muted">Comprobando sesión…</p>
    </section>
  </main>
{:else if !authUser}
  <main class="login-shell">
    <section class="panel login-card">
      <button class="icon theme-button" type="button" onclick={toggleTheme}>
        {theme === 'light_theme' ? 'Oscuro' : 'Claro'}
      </button>
      <p class="eyebrow">GENERAL TPV · BACKOFFICE</p>
      <h1>{authStatus && !authStatus.has_users ? 'Configuración inicial' : 'Acceso al backoffice'}</h1>
      {#if authStatus && !authStatus.has_users}
        <p class="muted">
          Todavía no hay usuarios centrales. Crea el administrador inicial; después podrás
          gestionar usuarios y terminales.
        </p>
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void bootstrap();
          }}
        >
          <label class="field">
            <span>Tenant</span>
            <input bind:value={bootstrapTenantId} placeholder="UUID del tenant (opcional)" autocomplete="off" />
          </label>
          <label class="field">
            <span>Usuario</span>
            <input bind:value={bootstrapUsername} autocomplete="username" />
          </label>
          <label class="field">
            <span>Nombre visible</span>
            <input bind:value={bootstrapDisplayName} />
          </label>
          <label class="field">
            <span>Contraseña</span>
            <input bind:value={bootstrapPassword} type="password" autocomplete="new-password" />
          </label>
          <button class="primary full" type="submit" disabled={authBusy}>
            {authBusy ? 'Creando…' : 'Crear administrador'}
          </button>
        </form>
      {:else}
        <p class="muted">Acceso reservado a administradores, responsables y auditores.</p>
        <form
          onsubmit={(event) => {
            event.preventDefault();
            void login();
          }}
        >
          <label class="field">
            <span>Usuario</span>
            <input bind:value={loginUsername} autocomplete="username" />
          </label>
          <label class="field">
            <span>Contraseña</span>
            <input bind:value={loginPassword} type="password" autocomplete="current-password" />
          </label>
          <button class="primary full" type="submit" disabled={authBusy}>
            {authBusy ? 'Comprobando…' : 'Entrar'}
          </button>
        </form>
      {/if}
      {#if authError}
        <p class="error" role="alert">{authError}</p>
      {/if}
    </section>
  </main>
{:else}
  <main class="shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">GENERAL TPV · CENTRAL</p>
      <h1>Consolidación multi-tienda</h1>
      <p class="muted">Eventos recibidos desde las cajas. Solo lectura, sin reescribir ventas.</p>
    </div>
    <div class="actions">
      <span class="muted small">{authUser?.display_name} · {authUser?.role}</span>
      <span class="status" class:ok={!error}>{error ? 'SIN CONEXIÓN' : 'CONECTADO'}</span>
      <button class="secondary" type="button" onclick={toggleTheme}>
        {theme === 'light_theme' ? 'Oscuro' : 'Claro'}
      </button>
      <button type="button" onclick={() => void load()} disabled={loading}>
        {loading ? 'Actualizando…' : 'Actualizar'}
      </button>
      <button class="secondary" type="button" onclick={() => void logout()}>Salir</button>
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
      <p class="eyebrow">USUARIOS</p>
      <strong>{activeUsers}</strong>
      <span>{centralUsers.length} centrales</span>
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
        <p class="eyebrow">USUARIOS</p>
        <h2>Usuarios centrales</h2>
      </div>
      <span class="muted small">{activeUsers} activos · {centralUsers.length} registrados</span>
    </div>

    {#if usersError}
      <p class="error" role="alert">{usersError}</p>
    {/if}

    {#if !canWrite}
      <p class="muted small">Modo solo lectura: se requiere rol de administrador para crear o modificar usuarios.</p>
    {/if}

    {#if canWrite}
    <form
      class="user-form"
      onsubmit={(event) => {
        event.preventDefault();
        void createCentralUser();
      }}
    >
      <label class="field">
        <span>Tenant</span>
        <input bind:value={newTenantId} placeholder="UUID del tenant" autocomplete="off" />
      </label>
      <label class="field">
        <span>Tiendas</span>
        <input bind:value={newStoreIds} placeholder="UUID, UUID" autocomplete="off" />
      </label>
      <label class="field">
        <span>Usuario</span>
        <input bind:value={newUsername} autocomplete="off" />
      </label>
      <label class="field">
        <span>Nombre visible</span>
        <input bind:value={newDisplayName} />
      </label>
      <label class="field">
        <span>Rol</span>
        <select bind:value={newUserRole}>
          <option value="cashier">Cajero</option>
          <option value="manager">Responsable</option>
          <option value="auditor">Auditor</option>
          <option value="admin">Administrador</option>
        </select>
      </label>
      <label class="field">
        <span>Contraseña inicial</span>
        <input bind:value={newUserPassword} type="password" autocomplete="new-password" />
      </label>
      <div class="form-actions">
        <button class="primary" type="submit" disabled={userCreating}>
          {userCreating ? 'Creando…' : 'Crear usuario'}
        </button>
      </div>
    </form>
    {/if}

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Usuario</th>
            <th>Nombre</th>
            <th>Rol</th>
            <th>Tiendas</th>
            <th>Estado</th>
            <th>Versión</th>
            <th>Acciones</th>
          </tr>
        </thead>
        <tbody>
          {#each centralUsers as user (user.id)}
            <tr>
              <td class="mono strong">{user.username}</td>
              <td>{user.display_name}</td>
              <td>{user.role}</td>
              <td class="mono small" title={user.store_ids.join(', ')}>
                {user.store_ids.map(shortId).join(', ')}
              </td>
              <td>
                <span class="pill" class:revoked={!user.active}>
                  {user.active ? 'Activo' : 'Inactivo'}
                </span>
              </td>
              <td class="mono">{user.version}</td>
              <td class="row-actions">
                {#if canWrite}
                  <button
                    class="secondary"
                    type="button"
                    disabled={busyUserId === user.id}
                    onclick={() => void toggleCentralUser(user)}
                  >
                    {user.active ? 'Desactivar' : 'Activar'}
                  </button>
                  <button
                    class="secondary"
                    type="button"
                    disabled={busyUserId === user.id}
                    onclick={() => void resetCentralUserPassword(user)}
                  >
                    Restablecer
                  </button>
                {:else}
                  <span class="muted">—</span>
                {/if}
              </td>
            </tr>
          {:else}
            <tr>
              <td colspan="7" class="empty">Todavía no hay usuarios centrales.</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
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
                {#if canWrite}
                  <button
                    class="danger"
                    type="button"
                    disabled={device.status === 'revoked' || busyDeviceId === device.device_id}
                    onclick={() => void revokeDevice(device)}
                  >
                    {busyDeviceId === device.device_id ? 'Revocando…' : 'Revocar'}
                  </button>
                {:else}
                  <span class="muted">—</span>
                {/if}
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
              <td class="num strong">{saleTotal(item.event)}</td>
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
{/if}

<style>
  :global(*),
  :global(*::before),
  :global(*::after) {
    box-sizing: border-box;
  }

  :global(body) {
    margin: 0;
    background:
      radial-gradient(1200px 600px at 15% -10%, var(--glow-1) 0%, transparent 55%),
      radial-gradient(900px 500px at 110% 10%, var(--glow-2) 0%, transparent 50%),
      var(--bg);
    color: var(--text);
    font-family: "Segoe UI", system-ui, sans-serif;
  }

  .shell {
    max-width: 1240px;
    margin: 0 auto;
    padding: 32px 24px 64px;
  }

  .login-shell {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 24px;
  }

  .login-card {
    width: min(440px, 100%);
    position: relative;
  }

  .login-card h1 {
    margin: 4px 0 8px;
  }

  .login-card form {
    margin-top: 16px;
  }

  .login-card .field {
    margin: 12px 0;
  }

  .theme-button {
    position: absolute;
    top: 14px;
    right: 14px;
  }

  .full {
    width: 100%;
    margin-top: 8px;
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
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 800;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  .muted {
    color: var(--muted);
  }

  .small {
    font-size: 0.82rem;
  }

  .actions {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }

  .status {
    border: 1px solid var(--danger-border);
    color: var(--danger);
    border-radius: 999px;
    padding: 6px 10px;
    font-size: 0.72rem;
    font-weight: 800;
  }

  .status.ok {
    border-color: var(--success-border);
    color: var(--success);
  }

  button {
    background: var(--accent);
    color: var(--on-accent);
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
    background: var(--button-danger-bg);
    color: var(--button-danger-text);
    padding: 7px 10px;
    font-size: 0.8rem;
  }

  button.secondary {
    background: var(--secondary-bg);
    color: var(--text);
    padding: 7px 10px;
    font-size: 0.8rem;
  }

  .user-form {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    margin-bottom: 16px;
    align-items: end;
  }

  .form-actions {
    grid-column: 1 / -1;
    display: flex;
    justify-content: flex-end;
  }

  .field {
    display: grid;
    gap: 5px;
    min-width: 0;
  }

  .field span {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .field input,
  .field select {
    width: 100%;
    min-width: 0;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    color: var(--text);
    padding: 9px 10px;
    font: inherit;
  }

  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .error {
    border: 1px solid var(--danger-border);
    background: var(--danger-soft);
    color: var(--danger);
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
    background: var(--panel);
    border: 1px solid var(--line);
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
    color: var(--muted);
    font-size: 0.8rem;
  }

  .panel {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 18px;
    padding: 18px;
    margin-top: 18px;
  }

  .panel-heading {
    display: flex;
    justify-content: space-between;
    flex-wrap: wrap;
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
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }

  th {
    color: var(--muted);
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
    border: 1px solid var(--success-border);
    background: var(--success-soft);
    color: var(--success);
    border-radius: 999px;
    padding: 3px 8px;
    font-size: 0.75rem;
  }

  .pill.revoked {
    border-color: var(--danger-border);
    background: var(--danger-soft);
    color: var(--danger);
  }

  .empty {
    color: var(--muted);
    text-align: center;
    padding: 28px;
  }

  @media (max-width: 1050px) {
    .user-form {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 900px) {
    .cards {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .topbar {
      flex-direction: column;
    }
  }

  @media (max-width: 640px) {
    .shell {
      padding: 20px 16px 40px;
    }

    .user-form,
    .cards {
      grid-template-columns: 1fr;
    }

    .form-actions button {
      width: 100%;
    }
  }
</style>
