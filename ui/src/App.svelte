<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    formatMoney,
    stateLabel,
    taxPercent,
    type AppInfo,
    type CashMovementKind,
    type CashSession,
    type Invoice,
    type LineKind,
    type MasterDataConflict,
    type PaymentMethod,
    type Product,
    type ProductSearchResult,
    type PrintSummary,
    type QuarantinedEvent,
    type SaleView,
    type SessionUser,
    type Store,
    type SyncStatus,
    type SyncSummary,
    type UserAccount,
    type UserRole
  } from './lib/api';

  let session = $state<SessionUser | null>(null);
  let loginUsername = $state('admin');
  let loginPassword = $state('admin123');
  let loginError = $state('');
  let loggingIn = $state(false);
  let info = $state<AppInfo | null>(null);
  let sale = $state<SaleView | null>(null);
  let query = $state('');
  let results = $state<ProductSearchResult[]>([]);
  let message = $state('Cargando TPV…');
  let busy = $state(false);
  let paymentOpen = $state(false);
  let paymentMethod = $state<PaymentMethod>('cash');
  let amountCents = $state(0);
  let receivedCents = $state(0);
  let invoice = $state<Invoice | null>(null);
  let printSummary = $state<PrintSummary | null>(null);
  let cash = $state<CashSession | null>(null);
  let countedCash = $state('0.00');
  let lastSales = $state<SaleView[]>([]);
  let syncStatus = $state<SyncStatus | null>(null);
  let syncQuarantine = $state<QuarantinedEvent[]>([]);
  let syncBusy = $state(false);
  let syncNowBusy = $state(false);
  let syncSummary = $state<SyncSummary | null>(null);
  let backupBusy = $state(false);
  let rectifyBusy = $state(false);
  let movementBusy = $state(false);
  let movementKind = $state<CashMovementKind>('expense');
  let movementAmount = $state('0,00');
  let movementReason = $state('');
  let usersOpen = $state(false);
  let users = $state<UserAccount[]>([]);
  let stores = $state<Store[]>([]);
  let userBusy = $state(false);
  let userError = $state('');
  let newUsername = $state('');
  let newDisplayName = $state('');
  let newRole = $state<UserRole>('cashier');
  let newStoreId = $state('');
  let newPassword = $state('');
  let resetPasswordDrafts = $state<Record<string, string>>({});
  let catalogOpen = $state(false);
  let catalogItems = $state<Product[]>([]);
  let catalogConflicts = $state<MasterDataConflict[]>([]);
  let catalogError = $state('');
  let catalogBusy = $state(false);
  let catalogFilter = $state('');
  let editingProductId = $state<string | null>(null);
  let productSku = $state('');
  let productBarcode = $state('');
  let productName = $state('');
  let productFamily = $state('');
  let productPrice = $state('0.00');
  let productTaxCode = $state('IVA21');
  let productTaxPercentBp = $state(2100);
  let productActive = $state(true);

  let change = $derived(Math.max(0, receivedCents - amountCents));
  let lastClose = $derived(cash?.close_history.at(-1) ?? null);
  let filteredCatalog = $derived(
    catalogItems.filter((product) => {
      const needle = catalogFilter.trim().toLowerCase();
      return (
        !needle ||
        product.name.toLowerCase().includes(needle) ||
        product.sku.toLowerCase().includes(needle)
      );
    })
  );

  onMount(() => {
    void restoreSession();
    const timer = window.setInterval(() => {
      if (!session) return;
      void (async () => {
        try {
          const current = await api.currentUser();
          if (!current) {
            session = null;
            loginError = 'Sesión cerrada por un cambio de permisos en la central.';
            return;
          }
          await refreshSync();
        } catch {
          await refreshSync();
        }
      })();
    }, 30_000);
    return () => window.clearInterval(timer);
  });

  async function restoreSession() {
    try {
      const current = await api.currentUser();
      if (current) {
        session = current;
        await load();
      } else {
        message = 'Introduce tus credenciales para abrir la caja.';
      }
    } catch (error) {
      message = String(error);
    }
  }

  async function login() {
    loggingIn = true;
    loginError = '';
    try {
      session = await api.login(loginUsername, loginPassword);
      loginPassword = '';
      await load();
    } catch (error) {
      loginError = String(error);
    } finally {
      loggingIn = false;
    }
  }

  async function logout() {
    try {
      await api.logout();
    } catch {
      // The local session is cleared even if the central audit is unavailable.
    }
    session = null;
    info = null;
    sale = null;
    cash = null;
    invoice = null;
    printSummary = null;
    results = [];
    lastSales = [];
    syncStatus = null;
    syncQuarantine = [];
    usersOpen = false;
    users = [];
    stores = [];
    resetPasswordDrafts = {};
    message = 'Sesión cerrada.';
  }

  function roleLabel(role: string): string {
    switch (role) {
      case 'admin':
        return 'Administrador';
      case 'manager':
        return 'Responsable';
      case 'cashier':
        return 'Cajero';
      case 'auditor':
        return 'Auditor';
      default:
        return role;
    }
  }

  function shortId(value: string): string {
    return value.length > 12 ? `${value.slice(0, 8)}…` : value;
  }

  function movementLabel(kind: CashMovementKind): string {
    switch (kind) {
      case 'expense':
        return 'Gasto';
      case 'withdrawal':
        return 'Retirada';
      case 'deposit':
        return 'Ingreso';
    }
  }

  function movementSignedAmount(kind: CashMovementKind, cents: number): string {
    return `${kind === 'deposit' ? '+' : '−'}${formatMoney(cents)}`;
  }

  async function load() {
    try {
      info = await api.bootstrap();
      cash = info.open_cash;
      await refreshSales();
      await refreshSync();
      message = `Caja lista · ${info.store_name}`;
    } catch (error) {
      message = `No se pudo conectar con el núcleo TPV: ${String(error)}`;
    }
  }

  async function refreshSales() {
    try {
      lastSales = await api.listSales();
    } catch {
      lastSales = [];
    }
  }

  async function refreshSync() {
    try {
      syncStatus = await api.syncStatus();
      syncQuarantine = await api.listFailedSyncEvents(5);
    } catch {
      syncStatus = null;
      syncQuarantine = [];
    }
  }

  async function retrySyncEvent(eventId: string) {
    if (syncBusy) return;
    syncBusy = true;
    try {
      const requeued = await api.retrySyncEvents([eventId]);
      message = `${requeued} evento(s) devueltos a la cola de sincronización.`;
      await refreshSync();
    } catch (error) {
      message = String(error);
    } finally {
      syncBusy = false;
    }
  }

  async function retryAllSync() {
    if (syncBusy) return;
    syncBusy = true;
    try {
      const requeued = await api.retryAllSyncEvents();
      message = `${requeued} evento(s) devueltos a la cola de sincronización.`;
      await refreshSync();
    } catch (error) {
      message = String(error);
    } finally {
      syncBusy = false;
    }
  }

  async function syncNow() {
    if (syncNowBusy) return;
    syncNowBusy = true;
    try {
      syncSummary = await api.syncNow();
      message = `Sincronizado: ${syncSummary.accepted} aceptados, ${syncSummary.users_applied} usuario(s), ${syncSummary.master_applied} maestro(s).`;
      await refreshSync();
      await refreshSales();
    } catch (error) {
      message = String(error);
    } finally {
      syncNowBusy = false;
    }
  }

  async function backupNow() {
    if (backupBusy) return;
    backupBusy = true;
    try {
      const backup = await api.backupNow();
      message = `Backup cifrado creado: ${backup.path} (${backup.bytes} bytes)`;
    } catch (error) {
      message = String(error);
    } finally {
      backupBusy = false;
    }
  }

  async function openUsers() {
    userError = '';
    usersOpen = true;
    await refreshUsers();
  }

  async function refreshUsers() {
    try {
      users = await api.listUsers();
      stores = await api.listStores();
      const firstStore = stores[0];
      if (!newStoreId && firstStore) {
        newStoreId = firstStore.id;
      }
    } catch (error) {
      userError = String(error);
    }
  }

  async function openCatalog() {
    catalogError = '';
    catalogOpen = true;
    await refreshCatalog();
  }

  async function refreshCatalog() {
    catalogBusy = true;
    catalogError = '';
    try {
      const [items, conflicts] = await Promise.all([
        api.listProducts(),
        api.listMasterDataConflicts()
      ]);
      catalogItems = items;
      catalogConflicts = conflicts;
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }

  function resetProductForm() {
    editingProductId = null;
    productSku = '';
    productBarcode = '';
    productName = '';
    productFamily = '';
    productPrice = '0.00';
    productTaxCode = 'IVA21';
    productTaxPercentBp = 2100;
    productActive = true;
    catalogError = '';
  }

  function editProduct(product: Product) {
    editingProductId = product.id;
    productSku = product.sku;
    productBarcode = product.barcode ?? '';
    productName = product.name;
    productFamily = product.family_code ?? '';
    productPrice = product.unit_price;
    productTaxCode = product.tax_rate.code;
    productTaxPercentBp = product.tax_rate.percent_bp;
    productActive = product.active;
    catalogError = '';
  }

  async function saveProduct() {
    if (catalogBusy) return;
    catalogError = '';
    if (!productSku.trim() || !productName.trim()) {
      catalogError = 'SKU y nombre son obligatorios.';
      return;
    }
    const price = Number.parseFloat(productPrice.replace(',', '.'));
    if (!Number.isFinite(price) || price < 0) {
      catalogError = 'El precio debe ser un número no negativo.';
      return;
    }
    catalogBusy = true;
    try {
      const saved = await api.saveProduct({
        product_id: editingProductId,
        sku: productSku.trim(),
        barcode: productBarcode.trim() || null,
        name: productName.trim(),
        family_code: productFamily.trim() || null,
        unit_price: productPrice.trim().replace(',', '.'),
        tax_code: productTaxCode.trim(),
        tax_percent_bp: productTaxPercentBp,
        active: productActive
      });
      catalogItems = catalogItems.some((item) => item.id === saved.id)
        ? catalogItems.map((item) => (item.id === saved.id ? saved : item))
        : [...catalogItems, saved];
      message = `Producto guardado: ${saved.sku} · ${saved.name}`;
      resetProductForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }

  async function createUser() {
    if (userBusy) return;
    userError = '';
    if (!newUsername.trim() || !newDisplayName.trim() || newPassword.length < 8) {
      userError = 'Usuario, nombre y contraseña (mínimo 8 caracteres) son obligatorios.';
      return;
    }
    userBusy = true;
    try {
      const created = await api.createUser(
        newUsername.trim(),
        newDisplayName.trim(),
        newRole,
        newStoreId ? [newStoreId] : [],
        newPassword
      );
      users = [...users, created];
      newUsername = '';
      newDisplayName = '';
      newRole = 'cashier';
      newPassword = '';
      message = `Usuario ${created.username} creado.`;
    } catch (error) {
      userError = String(error);
    } finally {
      userBusy = false;
    }
  }

  async function saveUser(user: UserAccount) {
    if (userBusy) return;
    userBusy = true;
    userError = '';
    try {
      const updated = await api.updateUser(
        user.id,
        user.display_name,
        user.role,
        user.store_ids,
        user.active
      );
      const index = users.findIndex((item) => item.id === updated.id);
      if (index >= 0) {
        users[index] = updated;
      }
      message = `Usuario ${updated.username} actualizado.`;
    } catch (error) {
      userError = String(error);
    } finally {
      userBusy = false;
    }
  }

  async function resetUserPassword(user: UserAccount) {
    const password = resetPasswordDrafts[user.id] ?? '';
    if (password.length < 8) {
      userError = 'La nueva contraseña debe tener al menos 8 caracteres.';
      return;
    }
    if (userBusy) return;
    userBusy = true;
    userError = '';
    try {
      await api.resetPassword(user.id, password);
      resetPasswordDrafts[user.id] = '';
      message = `Contraseña de ${user.username} restablecida.`;
    } catch (error) {
      userError = String(error);
    } finally {
      userBusy = false;
    }
  }

  function setResetPassword(userId: string, value: string) {
    resetPasswordDrafts[userId] = value;
  }

  function toggleStoreAccess(user: UserAccount, storeId: string, checked: boolean) {
    if (checked) {
      if (!user.store_ids.includes(storeId)) {
        user.store_ids = [...user.store_ids, storeId];
      }
    } else {
      user.store_ids = user.store_ids.filter((id) => id !== storeId);
    }
  }

  async function search() {
    if (!query.trim()) {
      results = [];
      return;
    }
    try {
      results = await api.searchProducts(query.trim());
    } catch (error) {
      message = String(error);
    }
  }

  async function addProduct(sku: string) {
    if (busy) return;
    busy = true;
    try {
      if (!sale) {
        sale = await api.startSale();
      }
      sale = await api.addLine(sale.sale.id, sku, '1', 'sale');
      query = '';
      results = [];
      message = `Línea añadida · ${formatMoney(sale.totals.gross)}`;
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function openPayment() {
    if (!sale || sale.totals.due <= 0) {
      message = 'La venta no tiene importe pendiente.';
      return;
    }
    paymentMethod = 'cash';
    amountCents = sale.totals.due;
    receivedCents = amountCents;
    paymentOpen = true;
  }

  function setMethod(method: PaymentMethod) {
    paymentMethod = method;
    receivedCents = amountCents;
  }

  async function applyPayment() {
    if (!sale || busy) return;
    busy = true;
    try {
      sale = await api.applyPayment(
        sale.sale.id,
        paymentMethod,
        amountCents,
        receivedCents
      );
      if (sale.totals.due > 0) {
        amountCents = sale.totals.due;
        receivedCents = amountCents;
        message = `Pago parcial aplicado. Pendiente: ${formatMoney(sale.totals.due)}`;
      } else {
        message = `Cobro completo. Cambio: ${formatMoney(sale.totals.change)}. Pulsa F10 para cerrar.`;
      }
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function confirmSale() {
    if (!sale || busy) return;
    if (sale.totals.due !== 0) {
      message = 'Falta cobrar el importe pendiente.';
      return;
    }
    busy = true;
    try {
      sale = await api.confirmSale(sale.sale.id);
      paymentOpen = false;
      printSummary = await api.printTicket(sale.sale.id, false);
      message = `Ticket ${sale.sale.ticket_number ?? ''} confirmado e impreso.`;
      await refreshSales();
      await refreshSync();
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function issueInvoice() {
    if (!sale || !sale.sale.ticket_number || busy) return;
    busy = true;
    try {
      const issued = await api.issueInvoice(sale.sale.id);
      invoice = issued;
      sale.sale.invoice_id = issued.id;
      await refreshSales();
      message = `Factura ${issued.series}-${String(issued.number).padStart(6, '0')} emitida.`;
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function rectifyInvoice(invoiceId: string) {
    if (rectifyBusy) return;
    const reason = window.prompt('Motivo de la rectificación');
    if (!reason?.trim()) return;
    rectifyBusy = true;
    try {
      const rectified = await api.rectifyInvoice(invoiceId, reason.trim());
      invoice = rectified;
      await refreshSales();
      message = `Rectificación ${rectified.series}-${String(rectified.number).padStart(6, '0')} emitida.`;
    } catch (error) {
      message = String(error);
    } finally {
      rectifyBusy = false;
    }
  }

  async function reprintTicket() {
    if (!sale || !sale.sale.ticket_number || busy) return;
    busy = true;
    try {
      printSummary = await api.printTicket(sale.sale.id, true);
      message = `Copia de ticket generada: ${printSummary.file}`;
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function openCash() {
    if (busy) return;
    busy = true;
    try {
      cash = await api.openCash();
      message = 'Caja abierta con fondo de 100,00 €.';
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function closeCash() {
    if (!cash || busy) return;
    const cents = Math.round(Number.parseFloat(countedCash.replace(',', '.')) * 100);
    if (!Number.isFinite(cents) || cents < 0) {
      message = 'Importe contado no válido.';
      return;
    }
    busy = true;
    try {
      const record = await api.closeCash(cash.id, cents, 0, 0);
      message = `Cierre realizado. Diferencia: ${formatMoney(record.differences.cash)}`;
      cash = null;
      await refreshSales();
      await refreshSync();
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function registerCashMovement() {
    if (!cash || movementBusy) return;
    const amount = Number.parseFloat(movementAmount.replace(',', '.'));
    if (!Number.isFinite(amount) || amount <= 0) {
      message = 'El importe del movimiento debe ser positivo.';
      return;
    }
    if (!movementReason.trim()) {
      message = 'El motivo del movimiento es obligatorio.';
      return;
    }
    const cents = Math.round(amount * 100);
    movementBusy = true;
    try {
      cash = await api.cashMovement(
        cash.id,
        movementKind,
        cents,
        movementReason.trim()
      );
      message = `Movimiento registrado: ${movementLabel(movementKind)} ${formatMoney(cents)}.`;
      movementAmount = '0,00';
      movementReason = '';
    } catch (error) {
      message = String(error);
    } finally {
      movementBusy = false;
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'F10') {
      event.preventDefault();
      if (paymentOpen) {
        if (sale && sale.totals.due > 0) {
          void applyPayment();
        } else {
          void confirmSale();
        }
      } else if (sale && sale.totals.due === 0) {
        void confirmSale();
      } else if (results[0]) {
        void addProduct(results[0].sku);
      }
    } else if (event.key === 'F5') {
      event.preventDefault();
      openPayment();
    } else if (event.key === 'Escape') {
      paymentOpen = false;
      invoice = null;
    }
  }

  function productLineTotal(product: Product): string {
    const unit = Number.parseFloat(product.unit_price);
    return Number.isFinite(unit) ? formatMoney(Math.round(unit * 100)) : product.unit_price;
  }

  function lineTotal(line: { unit_price: string; quantity: string }): string {
    const unit = Number.parseFloat(line.unit_price);
    const quantity = Number.parseFloat(line.quantity);
    if (!Number.isFinite(unit) || !Number.isFinite(quantity)) return '-';
    return formatMoney(Math.round(unit * quantity * 100));
  }

  function lineKind(kind: LineKind): string {
    return kind === 'return' ? 'D' : 'V';
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if !session}
  <main class="shell login-shell">
    <section class="panel login-card">
      <p class="eyebrow">GENERAL TPV · ACCESO</p>
      <h1>Identifícate</h1>
      <p class="muted">Cada venta y movimiento de caja queda a nombre del operador.</p>
      <form onsubmit={(event) => { event.preventDefault(); void login(); }}>
        <label class="field">
          <span>Usuario</span>
          <input bind:value={loginUsername} autocomplete="username" />
        </label>
        <label class="field">
          <span>Contraseña</span>
          <input bind:value={loginPassword} type="password" autocomplete="current-password" />
        </label>
        {#if loginError}
          <p class="error" role="alert">{loginError}</p>
        {/if}
        <button class="primary full" type="submit" disabled={loggingIn}>
          {loggingIn ? 'Comprobando…' : 'Entrar'}
        </button>
      </form>
      <p class="muted small demo-hint">Demo local: admin / admin123</p>
    </section>
  </main>
{:else}
<main class="shell">
  <header class="topbar">
    <div>
      <p class="eyebrow">GENERAL TPV · LOCAL-FIRST</p>
      <h1>{info?.store_name ?? 'Cargando…'}</h1>
    </div>
    <div class="topbar-meta">
      <span class="badge">CAJA 1</span>
      <span class="badge online">OFFLINE-FIRST</span>
      <span class="badge">{session.user.display_name} · {roleLabel(session.user.role)}</span>
      <span class="clock">{new Date().toLocaleTimeString('es-ES')}</span>
      {#if session.user.role === 'admin' || session.user.role === 'manager'}
        <button class="icon" type="button" onclick={() => void openCatalog()}>Catálogo</button>
        <button
          class="icon"
          type="button"
          disabled={backupBusy}
          onclick={() => void backupNow()}
        >
          {backupBusy ? 'Copiando…' : 'Backup'}
        </button>
      {/if}
      {#if session.user.role === 'admin'}
        <button class="icon" type="button" onclick={() => void openUsers()}>Usuarios</button>
      {/if}
      <button class="icon" type="button" onclick={() => void logout()}>Salir</button>
    </div>
  </header>

  {#if message}
    <p class="message" role="status">{message}</p>
  {/if}

  <section class="workspace">
    <section class="panel sale-panel">
      <div class="panel-heading">
        <div>
          <p class="eyebrow">MOSTRADOR</p>
          <h2>Venta actual</h2>
        </div>
        <div class="ticket-meta">
          {#if sale}
            <span class="badge">Ticket {sale.sale.ticket_number ?? '—'}</span>
            <span class="badge">{stateLabel(sale.sale.state)}</span>
          {:else}
            <span class="badge">Sin venta</span>
          {/if}
        </div>
      </div>

      <form class="search" onsubmit={(event) => { event.preventDefault(); void search(); }}>
        <input
          bind:value={query}
          oninput={() => void search()}
          placeholder="Código, nombre o código de barras…"
          aria-label="Buscar artículo"
          autocomplete="off"
        />
        <button class="primary" type="submit" disabled={busy}>Añadir</button>
      </form>

      {#if results.length > 0}
        <div class="results">
          {#each results as result (result.sku)}
            <button class="result" type="button" onclick={() => void addProduct(result.sku)}>
              <span><strong>{result.sku}</strong> · {result.name}</span>
              <span>{formatMoney(result.price_cents)}</span>
            </button>
          {/each}
        </div>
      {/if}

      <div class="lines">
        {#if sale && sale.sale.lines.length > 0}
          <table>
            <thead>
              <tr>
                <th>Código</th>
                <th>Descripción</th>
                <th>Unid.</th>
                <th>Precio</th>
                <th>Total</th>
                <th>Tipo</th>
              </tr>
            </thead>
            <tbody>
              {#each sale.sale.lines as line (line.id)}
                <tr>
                  <td class="mono">{line.sku}</td>
                  <td>{line.description}</td>
                  <td class="num">{line.quantity}</td>
                  <td class="num">{formatMoney(Math.round(Number(line.unit_price) * 100))}</td>
                  <td class="num strong">{lineTotal(line)}</td>
                  <td class="mono">{lineKind(line.kind)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <div class="empty">
            <p>Escanea o teclea un código para empezar.</p>
            <p class="hint">F10 añade la primera coincidencia · F5 abre el cobro</p>
          </div>
        {/if}
      </div>

      <div class="sale-footer">
        <div class="totals">
          {#if sale}
            {#each sale.totals.tax_breakdown as breakdown (breakdown.code)}
              <div class="row muted">
                <span>{breakdown.code} · IVA {taxPercent(breakdown.percent_bp)}%</span>
                <span>base {formatMoney(breakdown.net)} · cuota {formatMoney(breakdown.tax)}</span>
              </div>
            {/each}
            <div class="row total">
              <span>TOTAL</span>
              <strong>{formatMoney(sale.totals.gross)}</strong>
            </div>
            <div class="row">
              <span>Pendiente</span>
              <strong>{formatMoney(sale.totals.due)}</strong>
            </div>
            <div class="row muted">
              <span>Entregado / cambio</span>
              <span>{formatMoney(sale.totals.received)} / {formatMoney(sale.totals.change)}</span>
            </div>
          {:else}
            <div class="row total"><span>TOTAL</span><strong>0,00 €</strong></div>
          {/if}
        </div>
        <div class="actions">
          <button class="primary" type="button" disabled={!sale || busy} onclick={openPayment}>
            F5 · Cobrar
          </button>
          <button
            class="success"
            type="button"
            disabled={!sale || sale.totals.due !== 0 || busy}
            onclick={() => void confirmSale()}
          >
            F10 · Cerrar venta
          </button>
          <button
            class="secondary"
            type="button"
            disabled={!sale || !sale.sale.ticket_number || busy}
            onclick={() => void reprintTicket()}
          >
            Reimprimir ticket
          </button>
          <button
            class="secondary"
            type="button"
            disabled={!sale || !sale.sale.ticket_number || !!sale.sale.invoice_id || busy}
            onclick={() => void issueInvoice()}
          >
            Emitir factura
          </button>
        </div>
      </div>
    </section>

    <aside class="side">
      <section class="panel">
        <div class="panel-heading compact">
          <div>
            <p class="eyebrow">CATÁLOGO RÁPIDO</p>
            <h2>Artículos</h2>
          </div>
        </div>
        <div class="product-grid">
          {#each info?.products ?? [] as product (product.sku)}
            <button class="product" type="button" onclick={() => void addProduct(product.sku)}>
              <span class="mono">{product.sku}</span>
              <strong>{product.name}</strong>
              <span class="muted">{productLineTotal(product)}</span>
            </button>
          {/each}
        </div>
      </section>

      <section class="panel">
        <div class="panel-heading compact">
          <div>
            <p class="eyebrow">CAJA</p>
            <h2>Turno</h2>
          </div>
          <span class="badge">{cash ? 'ABIERTA' : 'CERRADA'}</span>
        </div>
        {#if cash}
          <p class="muted small">Fondo inicial: {formatMoney(cash.opening_float)}</p>
          {#if lastClose}
            <p class="muted small">Último cierre: {formatMoney(lastClose.expected.cash)} esperado · {formatMoney(lastClose.differences.cash)} diferencia</p>
          {/if}
          <div class="movement-form">
            <label class="field">
              <span>Tipo de movimiento</span>
              <select bind:value={movementKind}>
                <option value="expense">Gasto</option>
                {#if session.user.role === 'admin' || session.user.role === 'manager'}
                  <option value="withdrawal">Retirada</option>
                  <option value="deposit">Ingreso</option>
                {/if}
              </select>
            </label>
            <label class="field">
              <span>Importe del movimiento (€)</span>
              <input bind:value={movementAmount} inputmode="decimal" />
            </label>
            <label class="field">
              <span>Motivo del movimiento</span>
              <input bind:value={movementReason} />
            </label>
            <button
              class="secondary full"
              type="button"
              disabled={movementBusy}
              onclick={() => void registerCashMovement()}
            >
              Registrar movimiento
            </button>
          </div>
          {#if cash.movements.length > 0}
            <div class="movement-list">
              <p class="eyebrow">MOVIMIENTOS</p>
              {#each cash.movements as movement (movement.id)}
                <div class="movement-row">
                  <span>{movementLabel(movement.kind)}</span>
                  <span class="muted small">{movement.reason}</span>
                  <strong class:deposit={movement.kind === 'deposit'}>
                    {movementSignedAmount(movement.kind, movement.amount)}
                  </strong>
                </div>
              {/each}
            </div>
          {/if}
          <label class="field">
            <span>Efectivo contado (€)</span>
            <input bind:value={countedCash} inputmode="decimal" />
          </label>
          <button class="secondary full" type="button" disabled={busy} onclick={() => void closeCash()}>
            Cerrar caja
          </button>
        {:else}
          <p class="muted small">Abre la caja antes de operar.</p>
          <button class="primary full" type="button" disabled={busy} onclick={() => void openCash()}>
            Abrir caja · fondo 100,00 €
          </button>
        {/if}
      </section>

      <section class="panel sync-card" class:warning={!!syncStatus?.failed}>
        <div class="panel-heading compact">
          <div>
            <p class="eyebrow">SINCRONIZACIÓN</p>
            <h2>Cola local</h2>
          </div>
          <span class="badge" class:warn={!!syncStatus?.failed}>
            {syncStatus?.failed ? 'CUARENTENA' : 'AL DÍA'}
          </span>
        </div>
        {#if syncStatus}
          <div class="sync-counts">
            <div><strong>{syncStatus.pending}</strong><span>pendientes</span></div>
            <div><strong>{syncStatus.sent}</strong><span>enviados</span></div>
            <div><strong>{syncStatus.failed}</strong><span>fallidos</span></div>
          </div>
          {#if session?.user.role === 'admin' || session?.user.role === 'manager'}
            <button
              class="secondary full"
              type="button"
              disabled={syncNowBusy}
              onclick={() => void syncNow()}
            >
              {syncNowBusy ? 'Sincronizando…' : 'Sincronizar ahora'}
            </button>
            {#if syncSummary}
              <p class="muted small">
                {syncSummary.accepted} aceptados · {syncSummary.users_applied} usuario(s) · {syncSummary.master_applied} maestro(s)
              </p>
            {/if}
          {/if}
          {#if syncStatus.oldest_pending_at}
            <p class="muted small">
              Pendiente desde {new Date(syncStatus.oldest_pending_at).toLocaleString('es-ES')}
            </p>
          {/if}
          {#if syncStatus.last_error}
            <p class="muted small">Último error: {syncStatus.last_error}</p>
          {/if}
          {#if syncQuarantine.length > 0}
            <div class="quarantine-list">
              {#each syncQuarantine as entry (entry.event.event_id)}
                <div class="quarantine-item">
                  <span class="mono">{entry.event.event_type}</span>
                  <span class="muted small">
                    {entry.attempts} intento(s) · {entry.last_error ?? 'sin detalle'}
                  </span>
                  <button
                    class="secondary"
                    type="button"
                    disabled={syncBusy}
                    onclick={() => void retrySyncEvent(entry.event.event_id)}
                  >
                    Reintentar
                  </button>
                </div>
              {/each}
            </div>
            <button
              class="secondary full"
              type="button"
              disabled={syncBusy}
              onclick={() => void retryAllSync()}
            >
              Reintentar todos ({syncStatus.failed})
            </button>
          {:else}
            <p class="muted small">Sin eventos en cuarentena.</p>
          {/if}
        {:else}
          <p class="muted small">Estado de sincronización no disponible.</p>
        {/if}
      </section>

      <section class="panel">
        <div class="panel-heading compact">
          <div>
            <p class="eyebrow">ÚLTIMAS VENTAS</p>
            <h2>Histórico</h2>
          </div>
        </div>
        <div class="sales-list">
          {#each lastSales as item (item.sale.id)}
            <div class="sale-row">
              <span class="mono">#{item.sale.ticket_number ?? '—'}</span>
              <span>{stateLabel(item.sale.state)}</span>
              <strong>{formatMoney(item.totals.gross)}</strong>
              {#if item.sale.invoice_id && (session?.user.role === 'admin' || session?.user.role === 'manager')}
                <button
                  class="secondary"
                  type="button"
                  disabled={rectifyBusy}
                  onclick={() => {
                    if (item.sale.invoice_id) void rectifyInvoice(item.sale.invoice_id);
                  }}
                >
                  Rectificar
                </button>
              {:else}
                <span></span>
              {/if}
            </div>
          {:else}
            <p class="muted small">Todavía no hay ventas.</p>
          {/each}
        </div>
      </section>

      {#if printSummary}
        <section class="panel invoice-card">
          <p class="eyebrow">IMPRESIÓN</p>
          <h2>{printSummary.success ? 'Ticket generado' : 'Pendiente'}</h2>
          <p class="muted small">{printSummary.bytes} bytes · {printSummary.file}</p>
        </section>
      {/if}

      {#if invoice}
        <section class="panel invoice-card">
          <p class="eyebrow">FACTURA</p>
          <h2>{invoice.series}-{String(invoice.number).padStart(6, '0')}</h2>
          <p class="muted small">Total {formatMoney(invoice.totals.gross)} · IVA {formatMoney(invoice.totals.tax)}</p>
          <p class="muted small">Emitida {new Date(invoice.issued_at).toLocaleString('es-ES')}</p>
        </section>
      {/if}
    </aside>
  </section>

  {#if paymentOpen && sale}
    <div class="modal-backdrop" role="presentation">
      <div class="modal" role="dialog" aria-modal="true" aria-label="Forma de pago">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">VENTA</p>
            <h2>Forma de pago</h2>
          </div>
          <button class="icon" type="button" onclick={() => (paymentOpen = false)}>Esc</button>
        </div>

        <div class="payment-summary">
          <div class="row total"><span>Total a cobrar</span><strong>{formatMoney(sale.totals.gross)}</strong></div>
          <div class="row"><span>Total pagado</span><span>{formatMoney(sale.totals.paid)}</span></div>
          <div class="row"><span>Pendiente</span><strong>{formatMoney(sale.totals.due)}</strong></div>
        </div>

        <div class="methods">
          <button class:active={paymentMethod === 'cash'} type="button" onclick={() => setMethod('cash')}>Efectivo</button>
          <button class:active={paymentMethod === 'card'} type="button" onclick={() => setMethod('card')}>Tarjeta</button>
          <button class:active={paymentMethod === 'voucher'} type="button" onclick={() => setMethod('voucher')}>Vale</button>
        </div>

        <label class="field">
          <span>Importe a aplicar (céntimos)</span>
          <input bind:value={amountCents} type="number" min="1" step="1" />
        </label>
        <label class="field">
          <span>Entregado por el cliente (céntimos)</span>
          <input bind:value={receivedCents} type="number" min="1" step="1" />
        </label>
        <div class="row total"><span>Cambio</span><strong>{formatMoney(change)}</strong></div>

        <div class="modal-actions">
          <button class="primary" type="button" disabled={busy} onclick={() => void applyPayment()}>
            Aplicar pago
          </button>
          <button
            class="success"
            type="button"
            disabled={busy || sale.totals.due !== 0}
            onclick={() => void confirmSale()}
          >
            Cobrar y cerrar
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if usersOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal users-modal" role="dialog" aria-modal="true" aria-label="Gestión de usuarios">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">ADMINISTRACIÓN</p>
            <h2>Usuarios</h2>
          </div>
          <button class="icon" type="button" onclick={() => (usersOpen = false)}>Esc</button>
        </div>

        {#if userError}
          <p class="error" role="alert">{userError}</p>
        {/if}

        <form
          class="user-create"
          onsubmit={(event) => {
            event.preventDefault();
            void createUser();
          }}
        >
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
            <select bind:value={newRole}>
              <option value="admin">Administrador</option>
              <option value="manager">Responsable</option>
              <option value="cashier">Cajero</option>
              <option value="auditor">Auditor</option>
            </select>
          </label>
          <label class="field">
            <span>Tienda</span>
            <select bind:value={newStoreId}>
              <option value="">Sin tienda</option>
              {#each stores as store (store.id)}
                <option value={store.id}>{store.name}</option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span>Contraseña inicial</span>
            <input bind:value={newPassword} type="password" autocomplete="new-password" />
          </label>
          <button class="primary full" type="submit" disabled={userBusy}>
            Crear usuario
          </button>
        </form>

        <div class="user-list">
          {#each users as user (user.id)}
            <div class="user-row">
              <div class="user-heading">
                <span class="mono">{user.username}</span>
                <span class="badge">{user.active ? 'ACTIVO' : 'INACTIVO'}</span>
              </div>
              <div class="user-fields">
                <label class="field">
                  <span>Nombre visible</span>
                  <input bind:value={user.display_name} />
                </label>
                <label class="field">
                  <span>Rol</span>
                  <select bind:value={user.role}>
                    <option value="admin">Administrador</option>
                    <option value="manager">Responsable</option>
                    <option value="cashier">Cajero</option>
                    <option value="auditor">Auditor</option>
                  </select>
                </label>
                <div class="field">
                  <span>Estado</span>
                  <label class="inline">
                    <input type="checkbox" bind:checked={user.active} />
                    Activo
                  </label>
                </div>
              </div>
              <div class="user-stores">
                {#each stores as store (store.id)}
                  <label class="inline">
                    <input
                      type="checkbox"
                      checked={user.store_ids.includes(store.id)}
                      onchange={(event) =>
                        toggleStoreAccess(user, store.id, event.currentTarget.checked)}
                    />
                    {store.name}
                  </label>
                {/each}
              </div>
              <div class="user-actions">
                <button
                  class="secondary"
                  type="button"
                  disabled={userBusy}
                  onclick={() => void saveUser(user)}
                >
                  Guardar
                </button>
                <input
                  class="reset-input"
                  type="password"
                  placeholder="Nueva contraseña"
                  value={resetPasswordDrafts[user.id] ?? ''}
                  oninput={(event) => setResetPassword(user.id, event.currentTarget.value)}
                />
                <button
                  class="secondary"
                  type="button"
                  disabled={userBusy}
                  onclick={() => void resetUserPassword(user)}
                >
                  Restablecer
                </button>
              </div>
            </div>
          {:else}
            <p class="muted small">No hay usuarios.</p>
          {/each}
        </div>
      </div>
    </div>
  {/if}

  {#if catalogOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal catalog-modal" role="dialog" aria-modal="true" aria-label="Catálogo">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">DATOS MAESTROS</p>
            <h2>Catálogo</h2>
          </div>
          <button class="icon" type="button" onclick={() => (catalogOpen = false)}>Esc</button>
        </div>

        {#if catalogError}
          <p class="error" role="alert">{catalogError}</p>
        {/if}

        <form
          class="product-form"
          onsubmit={(event) => {
            event.preventDefault();
            void saveProduct();
          }}
        >
          <label class="field">
            <span>SKU</span>
            <input bind:value={productSku} autocomplete="off" />
          </label>
          <label class="field">
            <span>Nombre</span>
            <input bind:value={productName} />
          </label>
          <label class="field">
            <span>Código de barras</span>
            <input bind:value={productBarcode} inputmode="numeric" />
          </label>
          <label class="field">
            <span>Familia</span>
            <input bind:value={productFamily} />
          </label>
          <label class="field">
            <span>Precio</span>
            <input bind:value={productPrice} inputmode="decimal" />
          </label>
          <label class="field">
            <span>Código fiscal</span>
            <input bind:value={productTaxCode} />
          </label>
          <label class="field">
            <span>% IVA</span>
            <input bind:value={productTaxPercentBp} type="number" min="0" max="10000" step="1" />
          </label>
          <div class="field">
            <span>Estado</span>
            <label class="inline">
              <input type="checkbox" bind:checked={productActive} />
              Activo
            </label>
          </div>
          <div class="product-actions">
            <button class="primary" type="submit" disabled={catalogBusy}>
              {catalogBusy ? 'Guardando…' : 'Guardar producto'}
            </button>
            <button class="secondary" type="button" onclick={resetProductForm}>
              Nuevo producto
            </button>
          </div>
        </form>

        <div class="catalog-toolbar">
          <input
            class="catalog-filter"
            placeholder="Buscar producto"
            bind:value={catalogFilter}
            aria-label="Buscar producto"
          />
          <span class="muted small">{filteredCatalog.length} producto(s)</span>
        </div>

        <div class="product-list">
          {#each filteredCatalog as product (product.id)}
            <div class="product-row">
              <div>
                <strong>{product.name}</strong>
                <span class="muted small mono">
                  {product.sku} · {product.unit_price} € · {product.tax_rate.percent_bp / 100}% IVA
                </span>
              </div>
              <div class="product-row-actions">
                <span class="badge" class:inactive={!product.active}>
                  {product.active ? 'ACTIVO' : 'INACTIVO'}
                </span>
                <button class="secondary" type="button" onclick={() => editProduct(product)}>
                  Editar
                </button>
              </div>
            </div>
          {:else}
            <p class="muted small">No hay productos que coincidan.</p>
          {/each}
        </div>

        <div class="conflict-summary">
          <p class="eyebrow">CONFLICTOS</p>
          {#if catalogConflicts.length === 0}
            <p class="muted small">Sin conflictos de maestros.</p>
          {:else}
            {#each catalogConflicts as conflict (conflict.id)}
              <div class="conflict-row">
                <span>
                  {conflict.kind} · {shortId(conflict.entity_id)} · v{conflict.local_version} →
                  v{conflict.incoming.version}
                </span>
                <span class="badge warn">{conflict.reason}</span>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    </div>
  {/if}
</main>
{/if}

<style>
  .shell {
    max-width: 1500px;
    margin: 0 auto;
    padding: 24px;
  }

  .topbar {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 24px;
    margin-bottom: 18px;
  }

  .topbar h1 {
    margin: 2px 0 0;
    font-size: clamp(1.4rem, 2.2vw, 2rem);
  }

  .topbar-meta,
  .ticket-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 2.1fr) minmax(320px, 0.9fr);
    gap: 18px;
    align-items: start;
  }

  .side {
    display: grid;
    gap: 18px;
  }

  .panel {
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 18px;
    padding: 18px;
    box-shadow: 0 18px 50px rgb(0 0 0 / 18%);
  }

  .panel-heading {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
    margin-bottom: 16px;
  }

  .panel-heading.compact {
    margin-bottom: 10px;
  }

  .search {
    display: flex;
    gap: 10px;
  }

  .search input {
    flex: 1;
    font-size: 1.1rem;
    padding: 14px 16px;
  }

  .results {
    display: grid;
    gap: 6px;
    margin-top: 8px;
  }

  .result {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
    border-radius: 10px;
    padding: 10px 12px;
  }

  .lines {
    margin-top: 16px;
    min-height: 280px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.95rem;
  }

  th,
  td {
    padding: 10px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }

  th {
    color: var(--muted);
    font-weight: 600;
    font-size: 0.78rem;
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

  .empty {
    display: grid;
    place-items: center;
    min-height: 280px;
    text-align: center;
    border: 1px dashed var(--line);
    border-radius: 14px;
    color: var(--muted);
  }

  .hint {
    font-size: 0.8rem;
    color: var(--muted-2);
  }

  .sale-footer {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: flex-end;
    border-top: 1px solid var(--line);
    padding-top: 14px;
    margin-top: 4px;
  }

  .totals {
    min-width: 320px;
    flex: 1;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    padding: 3px 0;
  }

  .row.total {
    font-size: 1.25rem;
    padding: 8px 0;
  }

  .row.total strong {
    color: var(--accent);
  }

  .muted {
    color: var(--muted);
  }

  .small {
    font-size: 0.85rem;
  }

  .actions {
    display: grid;
    gap: 8px;
    min-width: 210px;
  }

  button {
    cursor: pointer;
    border-radius: 10px;
    border: 1px solid var(--line);
    padding: 11px 14px;
    font: inherit;
    font-weight: 650;
    transition: filter 120ms ease, transform 120ms ease;
  }

  button:hover:not(:disabled) {
    filter: brightness(1.12);
  }

  button:active:not(:disabled) {
    transform: translateY(1px);
  }

  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  button.primary {
    background: var(--accent);
    border-color: transparent;
    color: #0b1114;
  }

  button.success {
    background: var(--success);
    border-color: transparent;
    color: #07120c;
  }

  button.secondary,
  button.result,
  button.product,
  .methods button,
  button.icon {
    background: var(--panel-2);
    color: var(--text);
  }

  button.full {
    width: 100%;
    margin-top: 10px;
  }

  .product-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  button.product {
    display: grid;
    gap: 4px;
    text-align: left;
    padding: 10px;
  }

  button.product strong {
    font-size: 0.92rem;
  }

  .sales-list {
    display: grid;
    gap: 6px;
  }

  .sale-row {
    display: grid;
    grid-template-columns: 48px 1fr auto auto;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
    font-size: 0.88rem;
  }

  .invoice-card {
    border-color: rgb(112 224 161 / 45%);
  }

  .sync-card.warning {
    border-color: rgb(255 176 92 / 55%);
  }

  .badge.warn {
    background: rgb(255 176 92 / 18%);
    color: #ffb05c;
  }

  .movement-form {
    display: grid;
    gap: 8px;
    padding: 10px 0;
    margin: 10px 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }

  .movement-list {
    display: grid;
    gap: 6px;
    margin-bottom: 12px;
  }

  .movement-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
    font-size: 0.85rem;
  }

  .movement-row strong.deposit {
    color: #70e0a1;
  }

  .sync-counts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin-bottom: 10px;
  }

  .sync-counts div {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    text-align: center;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .sync-counts strong {
    font-size: 1.2rem;
  }

  .sync-counts span {
    color: var(--muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .quarantine-list {
    display: grid;
    gap: 6px;
    margin: 10px 0;
  }

  .quarantine-item {
    display: grid;
    gap: 4px;
    padding: 8px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .quarantine-item button {
    padding: 7px 10px;
  }

  .field {
    display: grid;
    gap: 6px;
    margin: 12px 0;
  }

  .field span {
    color: var(--muted);
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgb(3 8 10 / 72%);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 20;
  }

  .modal {
    width: min(560px, 100%);
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 18px;
    padding: 22px;
    box-shadow: 0 30px 80px rgb(0 0 0 / 45%);
  }

  .users-modal {
    width: min(980px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .user-create {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr)) auto;
    gap: 8px;
    align-items: end;
    padding-bottom: 14px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--line);
  }

  .user-create .field {
    margin: 0;
  }

  .user-create button {
    min-height: 40px;
    margin-top: 0;
  }

  .user-list {
    display: grid;
    gap: 10px;
  }

  .user-row {
    display: grid;
    gap: 10px;
    padding: 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .user-heading {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }

  .user-fields {
    display: grid;
    grid-template-columns: 1.4fr 1fr auto;
    gap: 10px;
    align-items: end;
  }

  .user-fields .field {
    margin: 0;
  }

  .user-stores {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .user-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }

  .user-actions button {
    padding: 8px 12px;
  }

  .reset-input {
    flex: 1;
    min-width: 180px;
    padding: 8px 10px;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.9rem;
  }

  .inline input {
    width: auto;
    padding: 0;
  }

  .catalog-modal {
    width: min(900px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .product-form {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    align-items: end;
    padding-bottom: 14px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .product-form .field {
    margin: 0;
  }

  .product-actions {
    grid-column: span 2;
    display: flex;
    gap: 8px;
    align-items: end;
  }

  .catalog-toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 10px;
  }

  .catalog-filter {
    flex: 1;
  }

  .product-list {
    display: grid;
    gap: 8px;
    max-height: 280px;
    overflow: auto;
  }

  .product-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .product-row > div:first-child {
    display: grid;
    gap: 3px;
  }

  .product-row-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .badge.inactive {
    background: #4b2c2c;
    color: #ffd8d8;
  }

  .conflict-summary {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }

  .conflict-row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 0;
  }

  .modal-heading {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
  }

  .payment-summary {
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 12px 14px;
    margin: 14px 0;
  }

  .methods {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin-bottom: 12px;
  }

  .methods button.active {
    background: var(--accent);
    color: #0b1114;
    border-color: transparent;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 18px;
  }

  .login-shell {
    min-height: 100vh;
    display: grid;
    place-items: center;
  }

  .login-card {
    width: min(420px, 100%);
  }

  .login-card h1 {
    margin: 4px 0 8px;
  }

  .login-card form {
    margin-top: 18px;
  }

  .error {
    color: var(--danger);
    font-size: 0.86rem;
    margin: 8px 0;
  }

  .demo-hint {
    margin-top: 14px;
    text-align: center;
  }

  @media (max-width: 980px) {
    .workspace {
      grid-template-columns: 1fr;
    }

    .sale-footer {
      flex-direction: column;
      align-items: stretch;
    }

    .totals {
      min-width: 0;
    }
  }
</style>
