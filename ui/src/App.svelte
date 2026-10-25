<script lang="ts">
  import { onMount } from 'svelte';
  import { applyTheme, nextTheme, storedTheme, type Theme } from './lib/theme';
  import {
    api,
    formatMoney,
    stateLabel,
    taxPercent,
    type AppInfo,
    type CashMovementKind,
    type CashSession,
    type Customer,
    type Family,
    type Invoice,
    type LineKind,
    type MasterDataConflict,
    type NoteInbox,
    type PaymentMethod,
    type PriceListSnapshot,
    type Product,
    type ProductImportReport,
    type ProductSearchResult,
    type PrintSummary,
    type QuarantinedEvent,
    type SaleHistoryPage,
    type SaleView,
    type SaveCustomerRequest,
    type SaveFamilyRequest,
    type SavePriceListRequest,
    type SaveSellerRequest,
    type SaveStoreRequest,
    type SaveTaxRequest,
    type Seller,
    type SessionUser,
    type StockLevel,
    type StockMovement,
    type StockMovementKind,
    type Store,
    type SyncStatus,
    type SyncSummary,
    type Tax,
    type TimeClockStatus,
    type UserAccount,
    type UserRole
  } from './lib/api';

  let theme = $state<Theme>('light_theme');
  let session = $state<SessionUser | null>(null);
  let loginUsername = $state('admin');
  let loginPassword = $state('admin123');
  let loginError = $state('');
  let loggingIn = $state(false);
  let info = $state<AppInfo | null>(null);
  let sale = $state<SaleView | null>(null);
  let query = $state('');
  let results = $state<ProductSearchResult[]>([]);
  let resultStock = $state<Record<string, StockLevel>>({});
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
  let historyOpen = $state(false);
  let historyPage = $state<SaleHistoryPage | null>(null);
  let historyBusy = $state(false);
  let historyError = $state('');
  let historyFrom = $state('');
  let historyTo = $state('');
  let historyState = $state('');
  let historyTicket = $state('');
  let historyOffset = $state(0);
  let historySelected = $state<SaleView | null>(null);
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
  let refundOpen = $state(false);
  let refundInvoice = $state<Invoice | null>(null);
  let refundReason = $state('');
  let refundMethod = $state<'cash' | 'card'>('cash');
  let refundReference = $state('');
  let refundQuantities = $state<Record<string, string>>({});
  let refundBusy = $state(false);
  let customers = $state<Customer[]>([]);
  let customerId = $state<string | null>(null);
  let customersOpen = $state(false);
  let customerBusy = $state(false);
  let customerError = $state('');
  let editCustomerId = $state<string | null>(null);
  let customerFirstName = $state('');
  let customerLastName1 = $state('');
  let customerLastName2 = $state('');
  let customerNif = $state('');
  let customerPhone = $state('');
  let customerEmail = $state('');
  let customerActive = $state(true);
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
  let catalogSection = $state<
    'products' | 'families' | 'taxes' | 'price-lists' | 'stock' | 'sellers' | 'stores'
  >('products');
  let families = $state<Family[]>([]);
  let taxes = $state<Tax[]>([]);
  let sellers = $state<Seller[]>([]);
  let editingFamilyId = $state<string | null>(null);
  let familyCode = $state('');
  let familyName = $state('');
  let familyActive = $state(true);
  let editingTaxId = $state<string | null>(null);
  let taxCode = $state('');
  let taxName = $state('');
  let taxRatePercent = $state('21');
  let taxActive = $state(true);
  let editingSellerId = $state<string | null>(null);
  let sellerCode = $state('');
  let sellerName = $state('');
  let sellerPhone1 = $state('');
  let sellerPhone2 = $state('');
  let sellerActive = $state(true);
  let editingStoreId = $state<string | null>(null);
  let storeCode = $state('');
  let storeName = $state('');
  let storeTaxId = $state('');
  let storePhone = $state('');
  let storeStreet = $state('');
  let storeNumber = $state('');
  let storeLocality = $state('');
  let storePostalCode = $state('');
  let storeProvince = $state('');
  let storeActive = $state(true);
  let editingProductId = $state<string | null>(null);
  let productSku = $state('');
  let productBarcode = $state('');
  let productName = $state('');
  let productFamily = $state('');
  let productPrice = $state('0.00');
  let productTaxCode = $state('IVA21');
  let productTaxPercentBp = $state(2100);
  let productActive = $state(true);
  let priceLists = $state<PriceListSnapshot[]>([]);
  let priceListId = $state('');
  let editingPriceListId = $state<string | null>(null);
  let priceListCode = $state('');
  let priceListName = $state('');
  let priceListActive = $state(true);
  let priceListPrices = $state<Record<string, string>>({});
  let priceListProductId = $state('');
  let priceListProductPrice = $state('');
  let stockLevels = $state<StockLevel[]>([]);
  let stockFilter = $state('');
  let stockStatusFilter = $state<'all' | 'low' | 'out'>('all');
  let lowStockAlertCount = $state(0);
  let stockSelectedProductId = $state<string | null>(null);
  let stockMovements = $state<StockMovement[]>([]);
  let stockMovementKind = $state<StockMovementKind>('purchase');
  let stockQuantity = $state('1');
  let stockReason = $state('');
  let stockMinQuantity = $state('0');
  let stockBusy = $state(false);
  let stockCountedQuantity = $state('');
  let stockCountReason = $state('');
  let stockCountBusy = $state(false);
  let stockExporting = $state(false);
  let stockMovementsExporting = $state(false);
  let stockLoading = $state(false);
  let importSourceSystem = $state('micro_server');
  let importBatchId = $state('');
  let importFileName = $state('');
  let importCsv = $state('');
  let importReport = $state<ProductImportReport | null>(null);
  let importError = $state('');
  let importBusy = $state(false);
  let importInput = $state<HTMLInputElement>();
  let timeClockOpen = $state(false);
  let timeClock = $state<TimeClockStatus | null>(null);
  let timeClockBusy = $state(false);
  let timeClockError = $state('');
  let timeClockTick = $state(Date.now());
  let notesOpen = $state(false);
  let noteInbox = $state<NoteInbox | null>(null);
  let noteBusy = $state(false);
  let noteError = $state('');
  let noteTitle = $state('');
  let noteBody = $state('');
  let noteRecipientIds = $state<string[]>([]);

  let change = $derived(Math.max(0, receivedCents - amountCents));
  let lastClose = $derived(cash?.close_history.at(-1) ?? null);
  let activeTimeClock = $derived(timeClock?.active ?? null);
  let unreadNotes = $derived(noteInbox?.unread ?? 0);
  let canManageNotes = $derived(
    session?.user.role === 'admin' || session?.user.role === 'manager'
  );
  let historyTotalPages = $derived(
    Math.max(1, Math.ceil((historyPage?.total ?? 0) / (historyPage?.limit || 25)))
  );
  let historyHasNext = $derived(
    historyOffset + (historyPage?.items.length ?? 0) < (historyPage?.total ?? 0)
  );
  let timeClockButtonLabel = $derived.by(() => {
    const active = activeTimeClock;
    const tick = timeClockTick;
    return active ? `Fichaje · ${formatDuration(active.entry.clock_in, active.entry.clock_out, tick)}` : 'Fichar';
  });
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

  let filteredStock = $derived(
    stockLevels.filter((level) => {
      if (stockStatusFilter === 'low' && !isStockLow(level)) return false;
      if (stockStatusFilter === 'out') {
        const quantity = Number.parseFloat(level.quantity);
        if (!Number.isFinite(quantity) || quantity > 0) return false;
      }
      const needle = stockFilter.trim().toLowerCase();
      return (
        !needle ||
        level.name.toLowerCase().includes(needle) ||
        level.sku.toLowerCase().includes(needle)
      );
    })
  );
  let selectedStockLevel = $derived(
    stockLevels.find((level) => level.product_id === stockSelectedProductId) ?? null
  );
  let lowStockCount = $derived(stockLevels.filter((level) => isStockLow(level)).length);
  let stockOutCount = $derived(
    stockLevels.filter((level) => {
      const quantity = Number.parseFloat(level.quantity);
      return Number.isFinite(quantity) && quantity <= 0;
    }).length
  );
  let stockTotalUnits = $derived(
    stockLevels
      .reduce((total, level) => {
        const quantity = Number.parseFloat(level.quantity);
        return total + (Number.isFinite(quantity) ? quantity : 0);
      }, 0)
      .toString()
  );
  let stockCountDifference = $derived.by(() => {
    if (!selectedStockLevel || !stockCountedQuantity.trim()) return null;
    const current = Number.parseFloat(selectedStockLevel.quantity);
    const counted = Number.parseFloat(stockCountedQuantity.trim().replace(',', '.'));
    if (!Number.isFinite(current) || !Number.isFinite(counted)) return null;
    const difference = counted - current;
    return Math.abs(difference) < 0.0005 ? '0' : difference.toFixed(3);
  });

  let selectableFamilies = $derived.by(() => {
    const options = families.filter(
      (family) => family.active || family.code === productFamily
    );
    if (productFamily && !families.some((family) => family.code === productFamily)) {
      options.unshift({
        id: `legacy-family-${productFamily}`,
        code: productFamily,
        name: 'Código sin maestro',
        active: false
      });
    }
    return options;
  });
  let selectableTaxes = $derived.by(() => {
    const options = taxes.filter((tax) => tax.active || tax.code === productTaxCode);
    if (productTaxCode && !taxes.some((tax) => tax.code === productTaxCode)) {
      options.unshift({
        id: `legacy-tax-${productTaxCode}`,
        code: productTaxCode,
        name: 'Código fiscal sin maestro',
        percent_bp: productTaxPercentBp,
        active: false
      });
    }
    return options;
  });

  let refundPreview = $derived.by(() => {
    if (!refundInvoice) return 0;
    let cents = 0;
    for (const line of refundInvoice.lines) {
      const raw = Number.parseFloat(
        (refundQuantities[line.source_line_id] ?? '0').replace(',', '.')
      );
      const max = Number.parseFloat(line.quantity);
      if (!Number.isFinite(raw) || raw <= 0 || !Number.isFinite(max) || max <= 0) {
        continue;
      }
      cents += Math.round((Math.abs(line.gross) * Math.min(raw, max)) / max);
    }
    return cents;
  });

  function toggleTheme() {
    theme = nextTheme(theme);
    applyTheme(theme);
  }

  onMount(() => {
    theme = storedTheme();
    applyTheme(theme);
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
          timeClockTick = Date.now();
          await refreshTimeClock();
          await refreshNotes();
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
    lowStockAlertCount = 0;
    priceListId = '';
    cash = null;
    invoice = null;
    printSummary = null;
    results = [];
    resultStock = {};
    lastSales = [];
    historyOpen = false;
    historyPage = null;
    historySelected = null;
    syncStatus = null;
    syncQuarantine = [];
    usersOpen = false;
    timeClockOpen = false;
    timeClock = null;
    notesOpen = false;
    noteInbox = null;
    noteTitle = '';
    noteBody = '';
    noteRecipientIds = [];
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

  function customerLabel(customer: Customer): string {
    return [customer.first_name, customer.last_name1, customer.last_name2]
      .filter(Boolean)
      .join(' ');
  }

  function dateTimeMillis(value: unknown): number {
    if (!value) return Number.NaN;
    if (Array.isArray(value) && value.length >= 6) {
      const [year, ordinal, hour, minute, second, nanosecond, offsetHours = 0, offsetMinutes = 0] =
        value as [number, number, number, number, number, number, number?, number?];
      const local = Date.UTC(year, 0, 1) + (ordinal - 1) * 86_400_000;
      const offset = (offsetHours * 60 + offsetMinutes) * 60_000;
      return new Date(
        local +
          hour * 3_600_000 +
          minute * 60_000 +
          second * 1_000 +
          Math.floor(nanosecond / 1_000_000) -
          offset
      ).getTime();
    }
    return new Date(String(value)).getTime();
  }

  function formatDateTime(value: unknown): string {
    const millis = dateTimeMillis(value);
    return Number.isNaN(millis) ? '—' : new Date(millis).toLocaleString('es-ES');
  }

  function localDateBoundary(date: string, endOfDay: boolean): number | null {
    if (!date) return null;
    const [year = 1970, month = 1, day = 1] = date.split('-').map(Number);
    const local = endOfDay
      ? new Date(year, month - 1, day, 23, 59, 59, 999)
      : new Date(year, month - 1, day, 0, 0, 0, 0);
    return local.getTime();
  }

  function paymentLabel(method: string): string {
    if (method === 'cash') return 'Efectivo';
    if (method === 'card') return 'Tarjeta';
    if (method === 'voucher') return 'Vale';
    if (method === 'credit') return 'Crédito';
    return method;
  }

  function resolveSellerName(id: string): string {
    return sellers.find((seller) => seller.id === id)?.name ?? id.slice(0, 8);
  }

  function resolveCustomerName(id: string | null): string {
    if (!id) return 'Sin cliente';
    const customer = customers.find((item) => item.id === id);
    return customer ? customerLabel(customer) : id.slice(0, 8);
  }

  function formatDuration(start: unknown, end: unknown, nowMillis: number): string {
    const startMillis = dateTimeMillis(start);
    const endMillis = end ? dateTimeMillis(end) : nowMillis;
    if (Number.isNaN(startMillis) || Number.isNaN(endMillis)) return '—';
    const totalMinutes = Math.max(0, Math.floor((endMillis - startMillis) / 60_000));
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return `${hours}h ${String(minutes).padStart(2, '0')}m`;
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
      customerId = info.customer_id;
      await refreshCustomers();
      await refreshSales();
      await refreshSync();
      await refreshTimeClock();
      await refreshNotes();
      await refreshLowStockAlert();
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

  async function searchSales(offset = 0) {
    if (historyBusy) return;
    const rawTicket = historyTicket.trim();
    let ticketNumber: number | null = null;
    if (rawTicket) {
      const parsed = Number(rawTicket);
      if (!Number.isInteger(parsed) || parsed <= 0) {
        historyError = 'El número de ticket debe ser un entero positivo.';
        return;
      }
      ticketNumber = parsed;
    }

    historyBusy = true;
    historyError = '';
    try {
      historyPage = await api.querySales({
        limit: 25,
        offset,
        from_ms: localDateBoundary(historyFrom, false),
        to_ms: localDateBoundary(historyTo, true),
        state: historyState || null,
        ticket_number: ticketNumber
      });
      historyOffset = offset;
      historySelected = null;
    } catch (error) {
      historyError = String(error);
    } finally {
      historyBusy = false;
    }
  }

  async function openHistory() {
    historyOpen = true;
    historySelected = null;
    historyError = '';
    await searchSales(0);
  }

  async function pageHistory(direction: -1 | 1) {
    const next = historyOffset + direction * 25;
    if (next < 0) return;
    await searchSales(next);
  }

  async function refreshCustomers() {
    try {
      customers = await api.listCustomers();
    } catch {
      customers = [];
    }
  }

  function resetCustomerForm() {
    editCustomerId = null;
    customerFirstName = '';
    customerLastName1 = '';
    customerLastName2 = '';
    customerNif = '';
    customerPhone = '';
    customerEmail = '';
    customerActive = true;
  }

  async function openCustomers() {
    customersOpen = true;
    customerError = '';
    resetCustomerForm();
    await refreshCustomers();
  }

  function editCustomer(customer: Customer) {
    editCustomerId = customer.id;
    customerFirstName = customer.first_name;
    customerLastName1 = customer.last_name1;
    customerLastName2 = customer.last_name2 ?? '';
    customerNif = customer.nif ?? '';
    customerPhone = customer.phone1 ?? '';
    customerEmail = customer.email ?? '';
    customerActive = customer.active;
    customerError = '';
  }

  async function saveCustomer() {
    if (customerBusy) return;
    if (!customerFirstName.trim() || !customerLastName1.trim()) {
      customerError = 'Nombre y primer apellido son obligatorios.';
      return;
    }
    customerBusy = true;
    customerError = '';
    try {
      const request: SaveCustomerRequest = {
        customer_id: editCustomerId,
        first_name: customerFirstName.trim(),
        last_name1: customerLastName1.trim(),
        last_name2: customerLastName2.trim() || null,
        nif: customerNif.trim() || null,
        phone: customerPhone.trim() || null,
        email: customerEmail.trim() || null,
        active: customerActive
      };
      const saved = await api.saveCustomer(request);
      await refreshCustomers();
      customerId = saved.id;
      message = `Cliente ${customerLabel(saved)} guardado.`;
      resetCustomerForm();
    } catch (error) {
      customerError = String(error);
    } finally {
      customerBusy = false;
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
      stores = (await api.listStores()).filter((store) => store.active);
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

  async function openLowStock() {
    catalogSection = 'stock';
    stockStatusFilter = 'low';
    stockFilter = '';
    await openCatalog();
  }

  async function refreshCatalog() {
    catalogBusy = true;
    catalogError = '';
    try {
      const [
        items,
        conflicts,
        familyItems,
        taxItems,
        sellerItems,
        storeItems,
        priceListItems,
        stockLevelItems
      ] = await Promise.all([
        api.listProducts(),
        api.listMasterDataConflicts(),
        api.listFamilies(),
        api.listTaxes(),
        api.listSellers(),
        api.listStores(),
        api.listPriceLists(),
        api.listStockLevels()
      ]);
      catalogItems = items;
      catalogConflicts = conflicts;
      families = familyItems;
      taxes = taxItems;
      sellers = sellerItems;
      stores = storeItems;
      priceLists = priceListItems;
      stockLevels = stockLevelItems;
      lowStockAlertCount = stockLevelItems.filter((level) => isStockLow(level)).length;
      const preferred =
        stockLevelItems.find((level) => level.product_id === stockSelectedProductId) ??
        stockLevelItems[0];
      if (preferred) {
        stockSelectedProductId = preferred.product_id;
        stockMinQuantity = preferred.min_quantity;
        stockCountedQuantity = preferred.quantity;
        stockCountReason = '';
        await loadStockMovements(preferred.product_id);
      }
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
      await refreshStockLevels(saved.id);
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }


  function isStockLow(level: StockLevel): boolean {
    const minimum = Number.parseFloat(level.min_quantity);
    return Number.isFinite(minimum) && minimum > 0 && Number.parseFloat(level.quantity) <= minimum;
  }

  async function refreshLowStockAlert() {
    if (!session || (session.user.role !== 'admin' && session.user.role !== 'manager')) {
      lowStockAlertCount = 0;
      return;
    }
    try {
      lowStockAlertCount = await api.countLowStock();
    } catch {
      lowStockAlertCount = 0;
    }
  }

  function formatQuantity(value: string): string {
    const parsed = Number.parseFloat(value);
    if (!Number.isFinite(parsed)) return value;
    return parsed.toLocaleString('es-ES', { maximumFractionDigits: 3 });
  }

  function formatSignedQuantity(value: string): string {
    const parsed = Number.parseFloat(value);
    if (!Number.isFinite(parsed) || parsed === 0) return formatQuantity(value);
    return `${parsed > 0 ? '+' : ''}${formatQuantity(value)}`;
  }

  function stockKindLabel(kind: StockMovementKind): string {
    switch (kind) {
      case 'initial':
        return 'Stock inicial';
      case 'purchase':
        return 'Entrada';
      case 'sale':
        return 'Venta';
      case 'return':
        return 'Devolución';
      case 'adjustment_in':
        return 'Ajuste positivo';
      case 'adjustment_out':
        return 'Ajuste negativo';
      default:
        return kind;
    }
  }

  async function selectStockLevel(level: StockLevel) {
    stockSelectedProductId = level.product_id;
    stockMinQuantity = level.min_quantity;
    stockReason = '';
    stockCountedQuantity = level.quantity;
    stockCountReason = '';
    await loadStockMovements(level.product_id);
  }

  async function loadStockMovements(productId: string) {
    stockLoading = true;
    catalogError = '';
    try {
      stockMovements = await api.listStockMovements(productId);
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockLoading = false;
    }
  }

  async function refreshStockLevels(preferredProductId = stockSelectedProductId) {
    const levels = await api.listStockLevels();
    stockLevels = levels;
    lowStockAlertCount = levels.filter((level) => isStockLow(level)).length;
    const preferred =
      levels.find((level) => level.product_id === preferredProductId) ?? levels[0];
    if (!preferred) {
      stockSelectedProductId = null;
      stockMovements = [];
      stockMinQuantity = '0';
      stockCountedQuantity = '';
      stockCountReason = '';
      return;
    }
    stockSelectedProductId = preferred.product_id;
    stockMinQuantity = preferred.min_quantity;
    stockCountedQuantity = preferred.quantity;
    stockCountReason = '';
    await loadStockMovements(preferred.product_id);
  }

  async function saveStockCount() {
    if (stockCountBusy || !stockSelectedProductId) return;
    stockCountBusy = true;
    catalogError = '';
    try {
      const result = await api.setStockCount({
        product_id: stockSelectedProductId,
        counted_quantity: stockCountedQuantity.trim(),
        reason: stockCountReason.trim()
      });
      message = Number.parseFloat(result.difference) === 0
        ? `Conteo sin diferencias: ${formatQuantity(result.level.quantity)} uds`
        : `Conteo físico ajustado: ${formatSignedQuantity(result.difference)} · existencias ${formatQuantity(result.level.quantity)}`;
      await refreshStockLevels(stockSelectedProductId);
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockCountBusy = false;
    }
  }

  async function recordStockMovement() {
    if (stockBusy || !stockSelectedProductId) return;
    stockBusy = true;
    catalogError = '';
    try {
      const movement = await api.recordStockMovement({
        product_id: stockSelectedProductId,
        kind: stockMovementKind,
        quantity: stockQuantity.trim(),
        reason: stockReason.trim() || null
      });
      message = `Movimiento de stock: ${stockKindLabel(movement.kind)} ${formatQuantity(movement.quantity)}`;
      stockQuantity = '1';
      stockReason = '';
      await refreshStockLevels(stockSelectedProductId);
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockBusy = false;
    }
  }

  async function saveStockMinimum() {
    if (stockBusy || !stockSelectedProductId) return;
    stockBusy = true;
    catalogError = '';
    try {
      const level = await api.setStockMinimum({
        product_id: stockSelectedProductId,
        min_quantity: stockMinQuantity.trim() || '0'
      });
      stockLevels = stockLevels.map((item) =>
        item.product_id === level.product_id ? level : item
      );
      stockMinQuantity = level.min_quantity;
      lowStockAlertCount = stockLevels.filter((item) => isStockLow(item)).length;
      message = `Mínimo de stock actualizado: ${formatQuantity(level.min_quantity)}`;
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockBusy = false;
    }
  }

  async function exportStockCsv() {
    if (stockExporting) return;
    stockExporting = true;
    catalogError = '';
    try {
      const path = await api.exportStockCsv();
      if (path) {
        message = `Stock exportado · ${path}`;
      }
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockExporting = false;
    }
  }

  async function exportStockMovements() {
    if (stockMovementsExporting || !stockSelectedProductId) return;
    stockMovementsExporting = true;
    catalogError = '';
    try {
      const path = await api.exportStockMovementsCsv(stockSelectedProductId);
      if (path) {
        message = `Historial exportado · ${path}`;
      }
    } catch (error) {
      catalogError = String(error);
    } finally {
      stockMovementsExporting = false;
    }
  }

  function resetFamilyForm() {
    editingFamilyId = null;
    familyCode = '';
    familyName = '';
    familyActive = true;
    catalogError = '';
  }

  function editFamily(family: Family) {
    editingFamilyId = family.id;
    familyCode = family.code;
    familyName = family.name;
    familyActive = family.active;
    catalogError = '';
  }

  async function saveFamily() {
    if (catalogBusy) return;
    catalogError = '';
    if (!familyCode.trim() || !familyName.trim()) {
      catalogError = 'Código y nombre de familia son obligatorios.';
      return;
    }
    const request: SaveFamilyRequest = {
      family_id: editingFamilyId,
      code: familyCode.trim(),
      name: familyName.trim(),
      active: familyActive
    };
    catalogBusy = true;
    try {
      const saved = await api.saveFamily(request);
      families = families.some((family) => family.id === saved.id)
        ? families.map((family) => (family.id === saved.id ? saved : family))
        : [...families, saved];
      message = `Familia guardada: ${saved.code} · ${saved.name}`;
      resetFamilyForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }

  function resetTaxForm() {
    editingTaxId = null;
    taxCode = '';
    taxName = '';
    taxRatePercent = '21';
    taxActive = true;
    catalogError = '';
  }

  function editTax(tax: Tax) {
    editingTaxId = tax.id;
    taxCode = tax.code;
    taxName = tax.name;
    taxRatePercent = (tax.percent_bp / 100).toFixed(2).replace(/\.00$/, '');
    taxActive = tax.active;
    catalogError = '';
  }

  function applyProductTax(code: string) {
    productTaxCode = code;
    const tax = taxes.find((item) => item.code === code);
    if (tax) {
      productTaxPercentBp = tax.percent_bp;
    }
  }

  async function saveTax() {
    if (catalogBusy) return;
    catalogError = '';
    if (!taxCode.trim() || !taxName.trim()) {
      catalogError = 'Código y nombre de impuesto son obligatorios.';
      return;
    }
    const percent = Number.parseFloat(taxRatePercent.replace(',', '.'));
    if (!Number.isFinite(percent) || percent < 0 || percent > 100) {
      catalogError = 'El IVA debe estar entre 0 y 100.';
      return;
    }
    const request: SaveTaxRequest = {
      tax_id: editingTaxId,
      code: taxCode.trim(),
      name: taxName.trim(),
      percent_bp: Math.round(percent * 100),
      active: taxActive
    };
    catalogBusy = true;
    try {
      const saved = await api.saveTax(request);
      taxes = taxes.some((tax) => tax.id === saved.id)
        ? taxes.map((tax) => (tax.id === saved.id ? saved : tax))
        : [...taxes, saved];
      message = `Impuesto guardado: ${saved.code} · ${saved.percent_bp / 100}%`;
      resetTaxForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }


  function resetSellerForm() {
    editingSellerId = null;
    sellerCode = '';
    sellerName = '';
    sellerPhone1 = '';
    sellerPhone2 = '';
    sellerActive = true;
    catalogError = '';
  }

  function editSeller(seller: Seller) {
    editingSellerId = seller.id;
    sellerCode = seller.code;
    sellerName = seller.name;
    sellerPhone1 = seller.phone1 ?? '';
    sellerPhone2 = seller.phone2 ?? '';
    sellerActive = seller.active;
    catalogError = '';
  }

  async function saveSeller() {
    if (catalogBusy) return;
    catalogError = '';
    if (!sellerCode.trim() || !sellerName.trim()) {
      catalogError = 'Código y nombre de vendedor son obligatorios.';
      return;
    }
    const request: SaveSellerRequest = {
      seller_id: editingSellerId,
      code: sellerCode.trim(),
      name: sellerName.trim(),
      phone1: sellerPhone1.trim() || null,
      phone2: sellerPhone2.trim() || null,
      active: sellerActive
    };
    catalogBusy = true;
    try {
      const saved = await api.saveSeller(request);
      sellers = sellers.some((seller) => seller.id === saved.id)
        ? sellers.map((seller) => (seller.id === saved.id ? saved : seller))
        : [...sellers, saved];
      message = `Vendedor guardado: ${saved.code} · ${saved.name}`;
      resetSellerForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }

  function resetStoreForm() {
    editingStoreId = null;
    storeCode = '';
    storeName = '';
    storeTaxId = '';
    storePhone = '';
    storeStreet = '';
    storeNumber = '';
    storeLocality = '';
    storePostalCode = '';
    storeProvince = '';
    storeActive = true;
    catalogError = '';
  }

  function editStore(store: Store) {
    editingStoreId = store.id;
    storeCode = store.code;
    storeName = store.name;
    storeTaxId = store.tax_id;
    storePhone = store.phone ?? '';
    storeStreet = store.address.street;
    storeNumber = store.address.number;
    storeLocality = store.address.locality;
    storePostalCode = store.address.postal_code;
    storeProvince = store.address.province;
    storeActive = store.active;
    catalogError = '';
  }

  async function saveStore() {
    if (catalogBusy) return;
    catalogError = '';
    if (!storeCode.trim() || !storeName.trim() || !storeTaxId.trim()) {
      catalogError = 'Código, nombre y NIF de tienda son obligatorios.';
      return;
    }
    const request: SaveStoreRequest = {
      store_id: editingStoreId,
      code: storeCode.trim(),
      name: storeName.trim(),
      tax_id: storeTaxId.trim(),
      phone: storePhone.trim() || null,
      street: storeStreet.trim(),
      number: storeNumber.trim(),
      locality: storeLocality.trim(),
      postal_code: storePostalCode.trim(),
      province: storeProvince.trim(),
      active: storeActive
    };
    catalogBusy = true;
    try {
      const saved = await api.saveStore(request);
      stores = stores.some((store) => store.id === saved.id)
        ? stores.map((store) => (store.id === saved.id ? saved : store))
        : [...stores, saved];
      message = `Tienda guardada: ${saved.code} · ${saved.name}`;
      resetStoreForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }


  function resetPriceListForm() {
    editingPriceListId = null;
    priceListCode = '';
    priceListName = '';
    priceListActive = true;
    priceListPrices = {};
    priceListProductId = '';
    priceListProductPrice = '';
    catalogError = '';
  }

  function editPriceList(snapshot: PriceListSnapshot) {
    editingPriceListId = snapshot.price_list.id;
    priceListCode = snapshot.price_list.code;
    priceListName = snapshot.price_list.name;
    priceListActive = snapshot.price_list.active;
    priceListPrices = Object.fromEntries(
      snapshot.items.map((item) => [item.product_id, item.unit_price])
    );
    priceListProductId = '';
    priceListProductPrice = '';
    catalogError = '';
  }

  function addPriceListItem() {
    catalogError = '';
    if (!priceListProductId) {
      catalogError = 'Selecciona un artículo para la tarifa.';
      return;
    }
    const price = Number.parseFloat(priceListProductPrice.trim().replace(',', '.'));
    if (!Number.isFinite(price) || price < 0) {
      catalogError = 'El precio de la tarifa debe ser un número no negativo.';
      return;
    }
    priceListPrices = {
      ...priceListPrices,
      [priceListProductId]: priceListProductPrice.trim().replace(',', '.')
    };
    priceListProductId = '';
    priceListProductPrice = '';
  }

  function removePriceListItem(productId: string) {
    const next = { ...priceListPrices };
    delete next[productId];
    priceListPrices = next;
  }

  async function savePriceList() {
    if (catalogBusy) return;
    catalogError = '';
    if (!priceListCode.trim() || !priceListName.trim()) {
      catalogError = 'Código y nombre de tarifa son obligatorios.';
      return;
    }
    const request: SavePriceListRequest = {
      price_list_id: editingPriceListId,
      code: priceListCode.trim(),
      name: priceListName.trim(),
      items: Object.entries(priceListPrices).map(([product_id, unit_price]) => ({
        product_id,
        unit_price: unit_price.replace(',', '.')
      })),
      active: priceListActive
    };
    catalogBusy = true;
    try {
      const saved = await api.savePriceList(request);
      priceLists = priceLists.some((item) => item.price_list.id === saved.price_list.id)
        ? priceLists.map((item) =>
            item.price_list.id === saved.price_list.id ? saved : item
          )
        : [...priceLists, saved];
      message = `Tarifa guardada: ${saved.price_list.code} · ${saved.price_list.name}`;
      resetPriceListForm();
    } catch (error) {
      catalogError = String(error);
    } finally {
      catalogBusy = false;
    }
  }

  async function selectImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    importFileName = file.name;
    importError = '';
    importReport = null;
    try {
      importCsv = await file.text();
      await previewImport();
    } catch (error) {
      importError = `No se pudo leer el fichero: ${String(error)}`;
    }
  }

  async function previewImport() {
    if (importBusy) return;
    importError = '';
    if (!importCsv.trim()) {
      importError = 'Selecciona un fichero CSV.';
      return;
    }
    if (!importSourceSystem.trim()) {
      importError = 'Indica el sistema de origen.';
      return;
    }
    importBusy = true;
    try {
      importReport = await api.previewProductImport(
        importSourceSystem.trim(),
        importBatchId.trim() || null,
        importCsv
      );
    } catch (error) {
      importError = String(error);
    } finally {
      importBusy = false;
    }
  }

  async function commitImport() {
    if (importBusy || !importReport) return;
    importError = '';
    importBusy = true;
    try {
      const report = await api.commitProductImport(
        importSourceSystem.trim(),
        importBatchId.trim() || null,
        importCsv
      );
      importReport = report;
      if (report.committed) {
        message = report.already_imported
          ? `Lote ${report.batch_id} ya importado; no se han duplicado datos.`
          : `Importación confirmada: ${report.inserted} nuevos, ${report.updated} actualizados.`;
        await refreshCatalog();
        try {
          info = await api.bootstrap();
        } catch {
          // The catalogue is already refreshed; a bootstrap race must not hide
          // the successful import report.
        }
      }
    } catch (error) {
      importError = String(error);
    } finally {
      importBusy = false;
    }
  }

  function resetImport() {
    importCsv = '';
    importFileName = '';
    importBatchId = '';
    importReport = null;
    importError = '';
    if (importInput) importInput.value = '';
  }

  async function refreshTimeClock() {
    try {
      timeClock = await api.timeClockStatus();
    } catch {
      // Background refresh must not interrupt the current sale.
    }
  }

  async function openTimeClock() {
    timeClockOpen = true;
    timeClockError = '';
    timeClockTick = Date.now();
    await refreshTimeClock();
  }

  async function clockIn() {
    if (timeClockBusy) return;
    timeClockBusy = true;
    timeClockError = '';
    try {
      timeClock = await api.timeClockIn();
      timeClockTick = Date.now();
      message = 'Entrada fichada.';
    } catch (error) {
      timeClockError = String(error);
    } finally {
      timeClockBusy = false;
    }
  }

  async function clockOut() {
    if (timeClockBusy) return;
    timeClockBusy = true;
    timeClockError = '';
    try {
      timeClock = await api.timeClockOut();
      timeClockTick = Date.now();
      message = 'Salida fichada.';
    } catch (error) {
      timeClockError = String(error);
    } finally {
      timeClockBusy = false;
    }
  }

  async function refreshNotes() {
    try {
      noteInbox = await api.noteList();
    } catch {
      // Notas no debe interrumpir la venta si el canal falla.
    }
  }

  async function openNotes() {
    notesOpen = true;
    noteError = '';
    await refreshNotes();
  }

  async function createNote() {
    if (noteBusy) return;
    noteError = '';
    if (!noteTitle.trim() || !noteBody.trim()) {
      noteError = 'Título y contenido son obligatorios.';
      return;
    }
    if (noteRecipientIds.length === 0) {
      noteError = 'Selecciona al menos un destinatario.';
      return;
    }
    noteBusy = true;
    try {
      noteInbox = await api.noteCreate({
        title: noteTitle.trim(),
        body: noteBody.trim(),
        recipient_ids: noteRecipientIds
      });
      noteTitle = '';
      noteBody = '';
      noteRecipientIds = [];
      message = 'Nota publicada.';
    } catch (error) {
      noteError = String(error);
    } finally {
      noteBusy = false;
    }
  }

  async function markNoteRead(noteId: string) {
    if (!noteId || noteBusy) return;
    noteBusy = true;
    noteError = '';
    try {
      noteInbox = await api.noteRead(noteId);
    } catch (error) {
      noteError = String(error);
    } finally {
      noteBusy = false;
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
      resultStock = {};
      return;
    }
    try {
      const found = await api.searchProducts(query.trim());
      if (found.length === 0) {
        results = [];
        resultStock = {};
        return;
      }
      let levels: StockLevel[] = [];
      try {
        levels = await api.listStockLevelsForProducts(found.map((result) => result.id));
      } catch (error) {
        message = String(error);
      }
      results = found;
      resultStock = Object.fromEntries(levels.map((level) => [level.product_id, level]));
    } catch (error) {
      message = String(error);
    }
  }

  async function addProduct(sku: string) {
    if (busy) return;
    busy = true;
    try {
      if (!sale) {
        sale = await api.startSale(customerId, priceListId || null);
      }
      sale = await api.addLine(sale.sale.id, sku, '1', 'sale');
      query = '';
      results = [];
      resultStock = {};
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
      await refreshLowStockAlert();
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

  async function openRefund(invoiceId: string) {
    if (refundBusy) return;
    try {
      const loaded = await api.getInvoice(invoiceId);
      refundInvoice = loaded;
      refundQuantities = Object.fromEntries(
        loaded.lines.map((line) => [line.source_line_id, '0'])
      );
      refundReason = '';
      refundMethod = cash ? 'cash' : 'card';
      refundReference = '';
      refundOpen = true;
      message = '';
    } catch (error) {
      message = String(error);
    }
  }

  async function submitRefund() {
    if (!refundInvoice || refundBusy) return;
    const reason = refundReason.trim();
    if (!reason) {
      message = 'El motivo de la devolución es obligatorio.';
      return;
    }
    const lines: { source_line_id: string; quantity: string }[] = [];
    for (const line of refundInvoice.lines) {
      const raw = (refundQuantities[line.source_line_id] ?? '0').replace(',', '.').trim();
      const quantity = Number.parseFloat(raw);
      const maximum = Number.parseFloat(line.quantity);
      if (!Number.isFinite(quantity) || quantity < 0) {
        message = `Cantidad no válida para ${line.description}.`;
        return;
      }
      if (quantity === 0) continue;
      if (quantity > maximum) {
        message = `La cantidad de ${line.description} supera la facturada.`;
        return;
      }
      lines.push({ source_line_id: line.source_line_id, quantity: raw });
    }
    if (lines.length === 0) {
      message = 'Selecciona al menos una cantidad a devolver.';
      return;
    }
    if (refundMethod === 'cash' && !cash) {
      message = 'Abre la caja para reembolsar en efectivo.';
      return;
    }
    const reference = refundReference.trim();
    if (refundMethod === 'card' && !reference) {
      message = 'El reembolso con tarjeta requiere la referencia del justificante.';
      return;
    }
    refundBusy = true;
    try {
      const summary = await api.refundInvoiceLines(
        refundInvoice.id,
        reason,
        refundMethod,
        reference || null,
        lines,
        cash?.id ?? null
      );
      invoice = summary.invoice;
      if (summary.cash) {
        cash = summary.cash;
      }
      message = `Devolución ${summary.invoice.series}-${String(summary.invoice.number).padStart(6, '0')} emitida.`;
      refundOpen = false;
      refundInvoice = null;
      await refreshSales();
      await refreshSync();
      await refreshLowStockAlert();
    } catch (error) {
      message = String(error);
    } finally {
      refundBusy = false;
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
      <button class="icon theme-button" type="button" onclick={toggleTheme}>
        {theme === 'light_theme' ? 'Oscuro' : 'Claro'}
      </button>
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
      <button class="icon" type="button" onclick={() => void openTimeClock()}>
        {timeClockButtonLabel}
      </button>
      <button class="icon notes-button" type="button" onclick={() => void openNotes()}>
        Notas
        {#if unreadNotes > 0}<span class="note-badge">{unreadNotes}</span>{/if}
      </button>
      <button class="icon" type="button" onclick={() => void openHistory()}>Consultas</button>
      {#if session.user.role === 'admin' || session.user.role === 'manager'}
        {#if lowStockAlertCount > 0}
          <button class="icon notes-button" type="button" onclick={() => void openLowStock()}>
            Stock bajo
            <span class="note-badge">{lowStockAlertCount}</span>
          </button>
        {/if}
        <button class="icon" type="button" onclick={() => void openCatalog()}>Catálogo</button>
        <button class="icon" type="button" onclick={() => void openCustomers()}>Clientes</button>
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
      <button class="icon" type="button" onclick={toggleTheme}>
        {theme === 'light_theme' ? 'Oscuro' : 'Claro'}
      </button>
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

      <div class="customer-picker">
        <label class="field">
          <span>Cliente</span>
          <select bind:value={customerId} disabled={!!sale}>
            {#each customers.filter((customer) => customer.active) as customer (customer.id)}
              <option value={customer.id}>
                {customerLabel(customer)}{customer.nif ? ` · ${customer.nif}` : ''}
              </option>
            {/each}
          </select>
        </label>
        <span class="muted small">El cliente se asigna al iniciar la venta.</span>
      </div>

      <div class="customer-picker">
        <label class="field">
          <span>Tarifa</span>
          <select bind:value={priceListId} disabled={!!sale}>
            <option value="">Precio base de catálogo</option>
            {#each priceLists.filter((list) => list.price_list.active) as list (list.price_list.id)}
              <option value={list.price_list.id}>
                {list.price_list.name} ({list.price_list.code})
              </option>
            {/each}
          </select>
        </label>
        <span class="muted small">La tarifa se fija al iniciar la venta.</span>
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
            {@const stock = resultStock[result.id]}
            <button class="result" type="button" onclick={() => void addProduct(result.sku)}>
              <span><strong>{result.sku}</strong> · {result.name}</span>
              {#if stock}
                <span
                  class="badge"
                  class:warn={isStockLow(stock)}
                  class:inactive={Number(stock.quantity) <= 0}
                >
                  {formatQuantity(stock.quantity)} uds
                </span>
              {:else}
                <span class="badge">—</span>
              {/if}
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
              Pendiente desde {formatDateTime(syncStatus.oldest_pending_at)}
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
                <div class="sale-actions">
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
                  <button
                    class="secondary"
                    type="button"
                    disabled={refundBusy}
                    onclick={() => {
                      if (item.sale.invoice_id) void openRefund(item.sale.invoice_id);
                    }}
                  >
                    Devolver
                  </button>
                </div>
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
          <p class="muted small">Emitida {formatDateTime(invoice.issued_at)}</p>
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

  {#if historyOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal history-modal" role="dialog" aria-modal="true" aria-label="Consultas de ventas">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">CONSULTAS</p>
            <h2>Histórico de ventas</h2>
          </div>
          <button class="icon" type="button" onclick={() => (historyOpen = false)}>Esc</button>
        </div>

        {#if historySelected}
          <div class="history-detail">
            <button class="icon" type="button" onclick={() => (historySelected = null)}>
              Volver
            </button>
            <div class="history-detail-head">
              <div>
                <h3>
                  Ticket #{historySelected.sale.ticket_number ?? '—'}
                  {#if historySelected.sale.ticket_series}
                    <span class="muted small">· {historySelected.sale.ticket_series}</span>
                  {/if}
                </h3>
                <p class="muted small">
                  {formatDateTime(historySelected.sale.created_at)} ·
                  {stateLabel(historySelected.sale.state)}
                </p>
              </div>
              <strong>{formatMoney(historySelected.totals.gross)}</strong>
            </div>

            <div class="history-meta">
              <span>Vendedor: {resolveSellerName(historySelected.sale.seller_id)}</span>
              <span>Cliente: {resolveCustomerName(historySelected.sale.customer_id)}</span>
            </div>

            <table class="history-lines">
              <thead>
                <tr>
                  <th>Artículo</th>
                  <th>Cantidad</th>
                  <th>Precio</th>
                  <th>IVA</th>
                </tr>
              </thead>
              <tbody>
                {#each historySelected.sale.lines as line (line.id)}
                  <tr>
                    <td>
                      <strong>{line.description}</strong>
                      <span class="muted small"> · {line.sku}</span>
                    </td>
                    <td>{line.quantity}</td>
                    <td>{line.unit_price} €</td>
                    <td>{line.tax_rate.percent_bp / 100}%</td>
                  </tr>
                {/each}
              </tbody>
            </table>

            <div class="history-totals">
              <div><span>Base</span><strong>{formatMoney(historySelected.totals.net)}</strong></div>
              <div><span>IVA</span><strong>{formatMoney(historySelected.totals.tax)}</strong></div>
              <div><span>Total</span><strong>{formatMoney(historySelected.totals.gross)}</strong></div>
              <div><span>Cobrado</span><strong>{formatMoney(historySelected.totals.paid)}</strong></div>
            </div>

            <div class="history-payments">
              <p class="eyebrow">PAGOS</p>
              {#each historySelected.sale.payments as payment (payment.id)}
                <div class="history-payment-row">
                  <span>{paymentLabel(payment.method)}</span>
                  <strong>{formatMoney(payment.amount)}</strong>
                </div>
              {:else}
                <p class="muted small">Sin pagos registrados.</p>
              {/each}
            </div>

            {#if historySelected.sale.invoice_id && (session?.user.role === 'admin' || session?.user.role === 'manager')}
              <div class="sale-actions history-detail-actions">
                <button
                  class="icon"
                  type="button"
                  disabled={rectifyBusy}
                  onclick={() => {
                    if (historySelected?.sale.invoice_id) void rectifyInvoice(historySelected.sale.invoice_id);
                  }}
                >
                  Rectificar factura
                </button>
                <button
                  class="icon"
                  type="button"
                  onclick={() => {
                    if (historySelected?.sale.invoice_id) void openRefund(historySelected.sale.invoice_id);
                  }}
                >
                  Devolver
                </button>
              </div>
            {/if}
          </div>
        {:else}
          <form
            class="history-filters"
            onsubmit={(event) => {
              event.preventDefault();
              void searchSales(0);
            }}
          >
            <label class="field">
              <span>Desde</span>
              <input type="date" bind:value={historyFrom} />
            </label>
            <label class="field">
              <span>Hasta</span>
              <input type="date" bind:value={historyTo} />
            </label>
            <label class="field">
              <span>Estado</span>
              <select bind:value={historyState}>
                <option value="">Todos</option>
                <option value="draft">Borrador</option>
                <option value="priced">Preciada</option>
                <option value="awaiting_payment">Pendiente de cobro</option>
                <option value="paid">Cobrada</option>
                <option value="closed">Cerrada</option>
                <option value="cancelled">Anulada</option>
              </select>
            </label>
            <label class="field">
              <span>Ticket</span>
              <input inputmode="numeric" bind:value={historyTicket} placeholder="Nº" />
            </label>
            <div class="history-filter-actions">
              <button class="primary" type="submit" disabled={historyBusy}>
                {historyBusy ? 'Consultando…' : 'Buscar'}
              </button>
              <button
                class="icon"
                type="button"
                onclick={() => {
                  historyFrom = '';
                  historyTo = '';
                  historyState = '';
                  historyTicket = '';
                  void searchSales(0);
                }}
              >
                Limpiar
              </button>
            </div>
          </form>

          {#if historyError}
            <p class="error" role="alert">{historyError}</p>
          {/if}

          <div class="history-summary">
            <div>
              <span class="eyebrow">VENTAS</span>
              <strong>{historyPage?.total ?? 0}</strong>
            </div>
            <div>
              <span class="eyebrow">BRUTO</span>
              <strong>{formatMoney(historyPage?.gross ?? 0)}</strong>
            </div>
            <div>
              <span class="eyebrow">IVA</span>
              <strong>{formatMoney(historyPage?.tax ?? 0)}</strong>
            </div>
            <div>
              <span class="eyebrow">COBRADO</span>
              <strong>{formatMoney(historyPage?.paid ?? 0)}</strong>
            </div>
          </div>

          <div class="history-list">
            {#each historyPage?.items ?? [] as item (item.sale.id)}
              <button
                class="history-row"
                type="button"
                onclick={() => (historySelected = item)}
              >
                <span class="mono">#{item.sale.ticket_number ?? '—'}</span>
                <span>{stateLabel(item.sale.state)}</span>
                <span class="muted small">{formatDateTime(item.sale.created_at)}</span>
                <strong>{formatMoney(item.totals.gross)}</strong>
              </button>
            {:else}
              <p class="muted small">No hay ventas para esos filtros.</p>
            {/each}
          </div>

          <div class="history-pager">
            <button
              class="icon"
              type="button"
              disabled={historyBusy || historyOffset === 0}
              onclick={() => void pageHistory(-1)}
            >
              Anterior
            </button>
            <span class="muted small">
              Página {Math.floor(historyOffset / 25) + 1} de {historyTotalPages}
            </span>
            <button
              class="icon"
              type="button"
              disabled={historyBusy || !historyHasNext}
              onclick={() => void pageHistory(1)}
            >
              Siguiente
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if notesOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal notes-modal" role="dialog" aria-modal="true" aria-label="Notas internas">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">COMUNICACIÓN DEL EQUIPO</p>
            <h2>Notas internas</h2>
          </div>
          <button class="icon" type="button" onclick={() => (notesOpen = false)}>Esc</button>
        </div>

        {#if noteError}
          <p class="error" role="alert">{noteError}</p>
        {/if}

        {#if canManageNotes}
          <section class="note-compose">
            <p class="eyebrow">NUEVA NOTA</p>
            <label class="field">
              <span>Título</span>
              <input bind:value={noteTitle} maxlength="120" placeholder="Cambio de turno" />
            </label>
            <label class="field">
              <span>Contenido</span>
              <textarea bind:value={noteBody} maxlength="4000" rows="4" placeholder="Instrucciones para el equipo"></textarea>
            </label>
            <fieldset class="note-recipients">
              <legend>Destinatarios</legend>
              {#each noteInbox?.recipients ?? [] as recipient (recipient.user_id)}
                <label class="note-recipient">
                  <input
                    type="checkbox"
                    bind:group={noteRecipientIds}
                    value={recipient.user_id}
                  />
                  {recipient.user_name}
                </label>
              {:else}
                <p class="muted small">No hay usuarios disponibles para esta tienda.</p>
              {/each}
            </fieldset>
            <button class="primary" type="button" disabled={noteBusy} onclick={() => void createNote()}>
              {noteBusy ? 'Publicando…' : 'Publicar nota'}
            </button>
          </section>
        {/if}

        <section class="note-list">
          <p class="eyebrow">BANDEJA</p>
          {#if noteInbox && noteInbox.notes.length > 0}
            {#each noteInbox.notes as view (view.note.id)}
              <article class="note-card" class:unread={!view.is_author && !view.viewer_read_at}>
                <div class="note-head">
                  <strong>{view.note.title}</strong>
                  {#if !view.is_author && !view.viewer_read_at}
                    <span class="badge note-unread">Sin leer</span>
                  {/if}
                </div>
                <p>{view.note.body}</p>
                <p class="muted small">
                  {view.author_name} · {formatDateTime(view.note.created_at)}
                </p>
                {#if view.recipients.length > 0}
                  <p class="muted small">
                    Para: {view.recipients.map((recipient) => recipient.user_name).join(', ')}
                    · {view.recipients.filter((recipient) => recipient.read_at).length}/{view.recipients.length} leídas
                  </p>
                {/if}
                {#if !view.is_author && !view.viewer_read_at}
                  <button
                    class="secondary"
                    type="button"
                    disabled={noteBusy}
                    onclick={() => void markNoteRead(view.note.id)}
                  >
                    Marcar leída
                  </button>
                {/if}
              </article>
            {/each}
          {:else}
            <p class="muted small">No hay notas en la bandeja.</p>
          {/if}
        </section>
      </div>
    </div>
  {/if}

  {#if timeClockOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal timeclock-modal" role="dialog" aria-modal="true" aria-label="Fichaje del personal">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">CONTROL DE PRESENCIA</p>
            <h2>Fichaje</h2>
          </div>
          <button class="icon" type="button" onclick={() => (timeClockOpen = false)}>Esc</button>
        </div>

        {#if timeClockError}
          <p class="error" role="alert">{timeClockError}</p>
        {/if}

        <section class="timeclock-current">
          <p class="eyebrow">ESTADO ACTUAL</p>
          {#if activeTimeClock}
            <p>
              <strong>{activeTimeClock.user_name}</strong> desde
              {formatDateTime(activeTimeClock.entry.clock_in)}
            </p>
            <p class="timeclock-elapsed">
              {formatDuration(activeTimeClock.entry.clock_in, activeTimeClock.entry.clock_out, timeClockTick)}
            </p>
            <button class="primary" type="button" disabled={timeClockBusy} onclick={() => void clockOut()}>
              {timeClockBusy ? 'Fichando…' : 'Fichar salida'}
            </button>
          {:else}
            <p class="muted">No tienes ningún fichaje abierto.</p>
            <button class="primary" type="button" disabled={timeClockBusy} onclick={() => void clockIn()}>
              {timeClockBusy ? 'Fichando…' : 'Fichar entrada'}
            </button>
          {/if}
        </section>

        <section class="timeclock-history">
          <p class="eyebrow">ÚLTIMOS FICHAJES</p>
          {#if timeClock && timeClock.recent.length > 0}
            <div class="timeclock-list">
              {#each timeClock.recent as view (view.entry.id)}
                <div class="timeclock-row">
                  <strong>{view.user_name}</strong>
                  <span>{formatDateTime(view.entry.clock_in)}</span>
                  <span>{view.entry.clock_out ? formatDateTime(view.entry.clock_out) : 'En curso'}</span>
                  <span class="mono">
                    {formatDuration(view.entry.clock_in, view.entry.clock_out, timeClockTick)}
                  </span>
                </div>
              {/each}
            </div>
          {:else}
            <p class="muted small">Todavía no hay fichajes registrados.</p>
          {/if}
        </section>
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

        <div class="catalog-tabs" role="tablist" aria-label="Secciones del catálogo">
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'products'}
            aria-selected={catalogSection === 'products'}
            onclick={() => (catalogSection = 'products')}
          >
            Artículos
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'stock'}
            aria-selected={catalogSection === 'stock'}
            onclick={() => (catalogSection = 'stock')}
          >
            Stock
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'families'}
            aria-selected={catalogSection === 'families'}
            onclick={() => (catalogSection = 'families')}
          >
            Familias
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'taxes'}
            aria-selected={catalogSection === 'taxes'}
            onclick={() => (catalogSection = 'taxes')}
          >
            Impuestos
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'price-lists'}
            aria-selected={catalogSection === 'price-lists'}
            onclick={() => (catalogSection = 'price-lists')}
          >
            Tarifas
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'sellers'}
            aria-selected={catalogSection === 'sellers'}
            onclick={() => (catalogSection = 'sellers')}
          >
            Vendedores
          </button>
          <button
            type="button"
            role="tab"
            class:active={catalogSection === 'stores'}
            aria-selected={catalogSection === 'stores'}
            onclick={() => (catalogSection = 'stores')}
          >
            Tiendas
          </button>
        </div>

        {#if catalogSection === 'products'}
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
            <select bind:value={productFamily}>
              <option value="">Sin familia</option>
              {#each selectableFamilies as family (family.id)}
                <option value={family.code}>
                  {family.name} ({family.code}){family.active ? '' : ' · inactiva'}
                </option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span>Precio</span>
            <input bind:value={productPrice} inputmode="decimal" />
          </label>
          <label class="field">
            <span>Código fiscal</span>
            <select
              value={productTaxCode}
              onchange={(event) => applyProductTax(event.currentTarget.value)}
            >
              <option value="">Selecciona IVA</option>
              {#each selectableTaxes as tax (tax.id)}
                <option value={tax.code}>
                  {tax.name} ({tax.percent_bp / 100}%){tax.active ? '' : ' · inactivo'}
                </option>
              {/each}
            </select>
          </label>
          <label class="field">
            <span>IVA (puntos básicos)</span>
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

        <div class="import-panel">
          <p class="eyebrow">MIGRACIÓN CSV</p>
          <p class="muted small">
            Obligatorias: source_id, sku, nombre, precio e IVA. Opcionales: código de barras,
            familia, código fiscal y activo. Se admite coma decimal y separadores ; , tabulador o |.
          </p>
          <div class="import-grid">
            <label class="field">
              <span>Sistema de origen</span>
              <input bind:value={importSourceSystem} autocomplete="off" />
            </label>
            <label class="field">
              <span>Lote (opcional)</span>
              <input bind:value={importBatchId} placeholder="Se calcula del contenido" autocomplete="off" />
            </label>
            <label class="field">
              <span>Fichero CSV</span>
              <input
                bind:this={importInput}
                type="file"
                accept=".csv,text/csv"
                onchange={selectImportFile}
              />
            </label>
            <div class="field">
              <span>Acciones</span>
              <div class="import-actions">
                <button
                  class="secondary"
                  type="button"
                  disabled={importBusy || !importCsv}
                  onclick={() => void previewImport()}
                >
                  {importBusy ? 'Procesando…' : 'Previsualizar'}
                </button>
                <button
                  class="primary"
                  type="button"
                  disabled={importBusy ||
                    !importReport ||
                    importReport.invalid_rows > 0 ||
                    importReport.committed}
                  onclick={() => void commitImport()}
                >
                  Confirmar importación
                </button>
                <button class="secondary" type="button" disabled={importBusy} onclick={resetImport}>
                  Limpiar
                </button>
              </div>
            </div>
          </div>
          {#if importFileName}
            <p class="muted small mono">{importFileName}</p>
          {/if}
          {#if importError}
            <p class="error" role="alert">{importError}</p>
          {/if}
          {#if importReport}
            <div class="import-report">
              <div class="import-summary">
                <span class="badge">Lote {importReport.batch_id}</span>
                <span class="muted small">
                  {importReport.total_rows} fila(s) · {importReport.valid_rows} válida(s) · {importReport.invalid_rows} con error
                </span>
              </div>
              <p class="small">
                Nuevos: <strong>{importReport.inserted}</strong> · Actualizados: <strong>{importReport.updated}</strong>
                · Sin cambios: <strong>{importReport.unchanged}</strong>
              </p>
              {#if importReport.already_imported}
                <p class="muted small">
                  Este lote ya estaba confirmado; no se han duplicado datos.
                </p>
              {/if}
              {#if importReport.errors.length > 0}
                <ul class="import-errors">
                  {#each importReport.errors as error}
                    <li>
                      Línea {error.line}{error.source_id ? ` · ${error.source_id}` : ''}: {error.message}
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if importReport.committed}
                <p><span class="badge ok">IMPORTACIÓN CONFIRMADA</span></p>
              {:else}
                <p class="muted small">Simulación: todavía no se ha escrito nada.</p>
              {/if}
            </div>
          {/if}
        </div>
        {/if}

        {#if catalogSection === 'stock'}
          <div class="stock-summary">
            <button
              class="stock-summary-card all"
              class:active={stockStatusFilter === 'all'}
              type="button"
              aria-pressed={stockStatusFilter === 'all'}
              onclick={() => (stockStatusFilter = 'all')}
            >
              <span class="muted small">Artículos</span>
              <strong>{stockLevels.length}</strong>
            </button>
            <button
              class="stock-summary-card low"
              class:active={stockStatusFilter === 'low'}
              type="button"
              aria-pressed={stockStatusFilter === 'low'}
              onclick={() => (stockStatusFilter = 'low')}
            >
              <span class="muted small">Bajo mínimo</span>
              <strong>{lowStockCount}</strong>
            </button>
            <button
              class="stock-summary-card out"
              class:active={stockStatusFilter === 'out'}
              type="button"
              aria-pressed={stockStatusFilter === 'out'}
              onclick={() => (stockStatusFilter = 'out')}
            >
              <span class="muted small">Sin stock</span>
              <strong>{stockOutCount}</strong>
            </button>
            <div class="stock-summary-card units">
              <span class="muted small">Unidades</span>
              <strong>{formatQuantity(stockTotalUnits)}</strong>
            </div>
          </div>

          <div class="stock-layout">
            <div class="stock-levels-panel">
              <div class="catalog-toolbar">
                <input
                  class="catalog-filter"
                  placeholder="Buscar stock"
                  bind:value={stockFilter}
                  aria-label="Buscar stock"
                />
                {#if session?.user.role === 'admin' || session?.user.role === 'manager'}
                  <button
                    class="secondary stock-export"
                    type="button"
                    disabled={stockExporting}
                    onclick={() => void exportStockCsv()}
                  >
                    {stockExporting ? 'Exportando…' : 'Exportar CSV'}
                  </button>
                {/if}
              </div>
              <div class="stock-level-list">
                {#each filteredStock as level (level.product_id)}
                  <button
                    type="button"
                    class="stock-level-row"
                    class:selected={level.product_id === stockSelectedProductId}
                    onclick={() => void selectStockLevel(level)}
                  >
                    <span class="stock-level-main">
                      <strong>{level.name}</strong>
                      <span class="muted small mono">
                        {level.sku} · mínimo {formatQuantity(level.min_quantity)}
                      </span>
                    </span>
                    <span class="stock-level-side">
                      <strong class:negative={Number.parseFloat(level.quantity) < 0}>
                        {formatQuantity(level.quantity)}
                      </strong>
                      {#if isStockLow(level)}
                        <span class="badge warn">BAJO</span>
                      {:else if Number.parseFloat(level.quantity) <= 0}
                        <span class="badge inactive">SIN STOCK</span>
                      {:else if Number.parseFloat(level.min_quantity) <= 0}
                        <span class="badge">SIN UMBRAL</span>
                      {:else}
                        <span class="badge ok">OK</span>
                      {/if}
                    </span>
                  </button>
                {:else}
                  <p class="muted small">
                    {#if stockStatusFilter === 'low'}
                      No hay artículos por debajo de su mínimo.
                    {:else if stockStatusFilter === 'out'}
                      No hay artículos sin existencias.
                    {:else}
                      No hay artículos que coincidan.
                    {/if}
                  </p>
                {/each}
              </div>
            </div>

            <div class="stock-detail-panel">
              {#if selectedStockLevel}
                <div class="stock-detail-heading">
                  <div>
                    <p class="eyebrow">EXISTENCIAS</p>
                    <h3>{selectedStockLevel.name}</h3>
                    <span class="muted small mono">{selectedStockLevel.sku}</span>
                  </div>
                  <div class="stock-current">
                    <strong class:negative={Number.parseFloat(selectedStockLevel.quantity) < 0}>
                      {formatQuantity(selectedStockLevel.quantity)}
                    </strong>
                    <span class="muted small">unidades</span>
                  </div>
                </div>

                {#if session?.user.role === 'admin' || session?.user.role === 'manager'}
                  <form
                    class="stock-form"
                    onsubmit={(event) => {
                      event.preventDefault();
                      void recordStockMovement();
                    }}
                  >
                    <label class="field">
                      <span>Tipo de movimiento</span>
                      <select bind:value={stockMovementKind} aria-label="Tipo de movimiento">
                        <option value="purchase">Entrada</option>
                        <option value="initial">Stock inicial</option>
                        <option value="adjustment_in">Ajuste positivo</option>
                        <option value="adjustment_out">Ajuste negativo</option>
                      </select>
                    </label>
                    <label class="field">
                      <span>Cantidad</span>
                      <input bind:value={stockQuantity} inputmode="decimal" aria-label="Cantidad" />
                    </label>
                    <label class="field">
                      <span>Motivo</span>
                      <input
                        bind:value={stockReason}
                        aria-label="Motivo"
                        placeholder="Compra, recuento, merma…"
                      />
                    </label>
                    <div class="product-actions">
                      <button class="primary" type="submit" disabled={stockBusy}>
                        {stockBusy ? 'Guardando…' : 'Registrar movimiento'}
                      </button>
                    </div>
                  </form>

                  <form
                    class="stock-min-form"
                    onsubmit={(event) => {
                      event.preventDefault();
                      void saveStockMinimum();
                    }}
                  >
                    <label class="field">
                      <span>Mínimo de stock</span>
                      <input
                        bind:value={stockMinQuantity}
                        inputmode="decimal"
                        aria-label="Mínimo de stock"
                      />
                    </label>
                    <button class="secondary" type="submit" disabled={stockBusy}>
                      Guardar mínimo
                    </button>
                  </form>

                  <p class="eyebrow stock-count-title">CONTEO FÍSICO</p>
                  <form
                    class="stock-count-form"
                    onsubmit={(event) => {
                      event.preventDefault();
                      void saveStockCount();
                    }}
                  >
                    <label class="field">
                      <span>Cantidad contada</span>
                      <input
                        bind:value={stockCountedQuantity}
                        inputmode="decimal"
                        aria-label="Conteo físico"
                        required
                      />
                    </label>
                    <label class="field">
                      <span>Motivo del conteo</span>
                      <input
                        bind:value={stockCountReason}
                        aria-label="Motivo del conteo"
                        placeholder="Recuento periódico, merma…"
                        required
                      />
                    </label>
                    <button
                      class="secondary"
                      type="submit"
                      disabled={stockCountBusy ||
                        stockCountDifference === null ||
                        !stockCountReason.trim()}
                    >
                      {stockCountBusy ? 'Ajustando…' : 'Ajustar a conteo'}
                    </button>
                  </form>
                  {#if stockCountDifference !== null}
                    <p class="muted small stock-count-preview">
                      {#if Number.parseFloat(stockCountDifference) === 0}
                        Sin diferencias respecto a las existencias actuales.
                      {:else}
                        Ajuste previsto:
                        <strong class:negative={Number.parseFloat(stockCountDifference) < 0}>
                          {formatSignedQuantity(stockCountDifference)}
                        </strong>
                        uds
                      {/if}
                    </p>
                  {/if}
                {:else}
                  <p class="muted small">
                    Solo un responsable puede registrar entradas o cambios de existencias.
                  </p>
                {/if}

                <div class="stock-movements">
                  <div class="stock-movements-heading">
                    <p class="eyebrow">ÚLTIMOS MOVIMIENTOS</p>
                    {#if session?.user.role === 'admin' || session?.user.role === 'manager'}
                      <button
                        class="secondary stock-history-export"
                        type="button"
                        disabled={stockMovementsExporting}
                        onclick={() => void exportStockMovements()}
                      >
                        {stockMovementsExporting ? 'Exportando…' : 'Exportar historial'}
                      </button>
                    {/if}
                  </div>
                  {#if stockLoading}
                    <p class="muted small">Cargando movimientos…</p>
                  {:else if stockMovements.length === 0}
                    <p class="muted small">Todavía no hay movimientos para este artículo.</p>
                  {:else}
                    {#each stockMovements as movement (movement.id)}
                      <div class="stock-movement-row">
                        <span>
                          <strong>{stockKindLabel(movement.kind)}</strong>
                          <span class="muted small">
                            {formatDateTime(movement.occurred_at)}
                            {movement.reason ? ` · ${movement.reason}` : ''}
                          </span>
                        </span>
                        <strong
                          class:negative={movement.kind === 'sale' ||
                            movement.kind === 'adjustment_out'}
                        >
                          {movement.kind === 'sale' || movement.kind === 'adjustment_out'
                            ? '−'
                            : '+'}{formatQuantity(movement.quantity)}
                        </strong>
                      </div>
                    {/each}
                  {/if}
                </div>
              {:else}
                <p class="muted small">Selecciona un artículo para ver sus existencias.</p>
              {/if}
            </div>
          </div>
        {/if}

        {#if catalogSection === 'families'}
          <form
            class="product-form"
            onsubmit={(event) => {
              event.preventDefault();
              void saveFamily();
            }}
          >
            <label class="field">
              <span>Código</span>
              <input bind:value={familyCode} autocomplete="off" placeholder="BEBIDAS" />
            </label>
            <label class="field">
              <span>Nombre</span>
              <input bind:value={familyName} />
            </label>
            <div class="field">
              <span>Estado</span>
              <label class="inline">
                <input type="checkbox" bind:checked={familyActive} />
                Activa
              </label>
            </div>
            <div class="product-actions">
              <button class="primary" type="submit" disabled={catalogBusy}>
                {catalogBusy ? 'Guardando…' : 'Guardar familia'}
              </button>
              <button class="secondary" type="button" onclick={resetFamilyForm}>
                Nueva familia
              </button>
            </div>
          </form>

          <div class="catalog-toolbar">
            <span class="muted small">{families.length} familia(s)</span>
          </div>

          <div class="product-list">
            {#each families as family (family.id)}
              <div class="product-row">
                <div>
                  <strong>{family.name}</strong>
                  <span class="muted small mono">{family.code}</span>
                </div>
                <div class="product-row-actions">
                  <span class="badge" class:inactive={!family.active}>
                    {family.active ? 'ACTIVA' : 'INACTIVA'}
                  </span>
                  <button class="secondary" type="button" onclick={() => editFamily(family)}>
                    Editar
                  </button>
                </div>
              </div>
            {:else}
              <p class="muted small">No hay familias.</p>
            {/each}
          </div>
        {/if}

        {#if catalogSection === 'taxes'}
          <form
            class="product-form"
            onsubmit={(event) => {
              event.preventDefault();
              void saveTax();
            }}
          >
            <label class="field">
              <span>Código</span>
              <input bind:value={taxCode} autocomplete="off" placeholder="IVA21" />
            </label>
            <label class="field">
              <span>Nombre</span>
              <input bind:value={taxName} />
            </label>
            <label class="field">
              <span>% IVA</span>
              <input bind:value={taxRatePercent} inputmode="decimal" />
            </label>
            <div class="field">
              <span>Estado</span>
              <label class="inline">
                <input type="checkbox" bind:checked={taxActive} />
                Activo
              </label>
            </div>
            <div class="product-actions">
              <button class="primary" type="submit" disabled={catalogBusy}>
                {catalogBusy ? 'Guardando…' : 'Guardar impuesto'}
              </button>
              <button class="secondary" type="button" onclick={resetTaxForm}>
                Nuevo impuesto
              </button>
            </div>
          </form>

          <div class="catalog-toolbar">
            <span class="muted small">{taxes.length} impuesto(s)</span>
          </div>

          <div class="product-list">
            {#each taxes as tax (tax.id)}
              <div class="product-row">
                <div>
                  <strong>{tax.name}</strong>
                  <span class="muted small mono">{tax.code} · {tax.percent_bp / 100}% IVA</span>
                </div>
                <div class="product-row-actions">
                  <span class="badge" class:inactive={!tax.active}>
                    {tax.active ? 'ACTIVO' : 'INACTIVO'}
                  </span>
                  <button class="secondary" type="button" onclick={() => editTax(tax)}>
                    Editar
                  </button>
                </div>
              </div>
            {:else}
              <p class="muted small">No hay impuestos.</p>
            {/each}
          </div>
        {/if}

        {#if catalogSection === 'sellers'}
          <form
            class="product-form"
            onsubmit={(event) => {
              event.preventDefault();
              void saveSeller();
            }}
          >
            <label class="field">
              <span>Código</span>
              <input bind:value={sellerCode} autocomplete="off" placeholder="V01" />
            </label>
            <label class="field">
              <span>Nombre</span>
              <input bind:value={sellerName} />
            </label>
            <label class="field">
              <span>Teléfono 1</span>
              <input bind:value={sellerPhone1} autocomplete="off" />
            </label>
            <label class="field">
              <span>Teléfono 2</span>
              <input bind:value={sellerPhone2} autocomplete="off" />
            </label>
            <div class="field">
              <span>Estado</span>
              <label class="inline">
                <input type="checkbox" bind:checked={sellerActive} />
                Activo
              </label>
            </div>
            <div class="product-actions">
              <button class="primary" type="submit" disabled={catalogBusy}>
                {catalogBusy ? 'Guardando…' : 'Guardar vendedor'}
              </button>
              <button class="secondary" type="button" onclick={resetSellerForm}>
                Nuevo vendedor
              </button>
            </div>
          </form>

          <div class="catalog-toolbar">
            <span class="muted small">{sellers.length} vendedor(es)</span>
          </div>

          <div class="product-list">
            {#each sellers as seller (seller.id)}
              <div class="product-row">
                <div>
                  <strong>{seller.name}</strong>
                  <span class="muted small mono">
                    {seller.code}{seller.phone1 ? ` · ${seller.phone1}` : ''}
                  </span>
                </div>
                <div class="product-row-actions">
                  <span class="badge" class:inactive={!seller.active}>
                    {seller.active ? 'ACTIVO' : 'INACTIVO'}
                  </span>
                  <button class="secondary" type="button" onclick={() => editSeller(seller)}>
                    Editar
                  </button>
                </div>
              </div>
            {:else}
              <p class="muted small">No hay vendedores.</p>
            {/each}
          </div>
        {/if}

        {#if catalogSection === 'stores'}
          <form
            class="product-form"
            onsubmit={(event) => {
              event.preventDefault();
              void saveStore();
            }}
          >
            <label class="field">
              <span>Código</span>
              <input bind:value={storeCode} autocomplete="off" placeholder="BEN01" />
            </label>
            <label class="field">
              <span>Nombre</span>
              <input bind:value={storeName} />
            </label>
            <label class="field">
              <span>NIF</span>
              <input bind:value={storeTaxId} autocomplete="off" placeholder="B12345674" />
            </label>
            <label class="field">
              <span>Teléfono</span>
              <input bind:value={storePhone} autocomplete="off" />
            </label>
            <label class="field">
              <span>Calle</span>
              <input bind:value={storeStreet} />
            </label>
            <label class="field">
              <span>Número</span>
              <input bind:value={storeNumber} />
            </label>
            <label class="field">
              <span>Localidad</span>
              <input bind:value={storeLocality} />
            </label>
            <label class="field">
              <span>Código postal</span>
              <input bind:value={storePostalCode} />
            </label>
            <label class="field">
              <span>Provincia</span>
              <input bind:value={storeProvince} />
            </label>
            <div class="field">
              <span>Estado</span>
              <label class="inline">
                <input type="checkbox" bind:checked={storeActive} />
                Activa
              </label>
            </div>
            <div class="product-actions">
              <button class="primary" type="submit" disabled={catalogBusy}>
                {catalogBusy ? 'Guardando…' : 'Guardar tienda'}
              </button>
              <button class="secondary" type="button" onclick={resetStoreForm}>
                Nueva tienda
              </button>
            </div>
          </form>

          <div class="catalog-toolbar">
            <span class="muted small">{stores.length} tienda(s)</span>
          </div>

          <div class="product-list">
            {#each stores as store (store.id)}
              <div class="product-row">
                <div>
                  <strong>{store.name}</strong>
                  <span class="muted small mono">
                    {store.code} · {store.tax_id}{store.phone ? ` · ${store.phone}` : ''}
                  </span>
                  <span class="muted small">
                    {store.address.street} {store.address.number}, {store.address.postal_code}{' '}
                    {store.address.locality} ({store.address.province})
                  </span>
                </div>
                <div class="product-row-actions">
                  <span class="badge" class:inactive={!store.active}>
                    {store.active ? 'ACTIVA' : 'INACTIVA'}
                  </span>
                  <button class="secondary" type="button" onclick={() => editStore(store)}>
                    Editar
                  </button>
                </div>
              </div>
            {:else}
              <p class="muted small">No hay tiendas.</p>
            {/each}
          </div>
        {/if}

        {#if catalogSection === 'price-lists'}
          <form
            class="product-form"
            onsubmit={(event) => {
              event.preventDefault();
              void savePriceList();
            }}
          >
            <label class="field">
              <span>Código</span>
              <input bind:value={priceListCode} autocomplete="off" placeholder="EMPLEADOS" />
            </label>
            <label class="field">
              <span>Nombre</span>
              <input bind:value={priceListName} placeholder="Tarifa de empleados" />
            </label>
            <div class="field">
              <span>Estado</span>
              <label class="inline">
                <input type="checkbox" bind:checked={priceListActive} />
                Activa
              </label>
            </div>
            <div class="product-actions">
              <button class="primary" type="submit" disabled={catalogBusy}>
                {catalogBusy ? 'Guardando…' : 'Guardar tarifa'}
              </button>
              <button class="secondary" type="button" onclick={resetPriceListForm}>
                Nueva tarifa
              </button>
            </div>
          </form>

          <div class="catalog-toolbar">
            <div class="price-list-item-form">
              <label class="field">
                <span>Artículo</span>
                <select bind:value={priceListProductId}>
                  <option value="">Selecciona artículo</option>
                  {#each catalogItems as product (product.id)}
                    <option value={product.id}>
                      {product.sku} · {product.name}
                    </option>
                  {/each}
                </select>
              </label>
              <label class="field">
                <span>Precio</span>
                <input bind:value={priceListProductPrice} inputmode="decimal" placeholder="0,00" />
              </label>
              <button class="secondary" type="button" onclick={addPriceListItem}>
                Añadir precio
              </button>
            </div>
            <span class="muted small">{Object.keys(priceListPrices).length} precio(s)</span>
          </div>

          {#if Object.keys(priceListPrices).length > 0}
            <div class="product-list">
              {#each Object.entries(priceListPrices) as [productId, price] (productId)}
                <div class="product-row">
                  <div>
                    <strong>
                      {catalogItems.find((product) => product.id === productId)?.name ?? productId.slice(0, 8)}
                    </strong>
                    <span class="muted small mono">
                      {catalogItems.find((product) => product.id === productId)?.sku ?? '—'}
                    </span>
                  </div>
                  <div class="product-row-actions">
                    <strong>{formatMoney(Math.round(Number(price) * 100))}</strong>
                    <button
                      class="secondary"
                      type="button"
                      onclick={() => removePriceListItem(productId)}
                    >
                      Quitar
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {:else}
            <p class="muted small">
              Añade precios concretos; los artículos sin precio usan el precio base del catálogo.
            </p>
          {/if}

          <div class="catalog-toolbar">
            <span class="muted small">{priceLists.length} tarifa(s)</span>
          </div>

          <div class="product-list">
            {#each priceLists as snapshot (snapshot.price_list.id)}
              <div class="product-row">
                <div>
                  <strong>{snapshot.price_list.name}</strong>
                  <span class="muted small mono">
                    {snapshot.price_list.code} · {snapshot.items.length} precio(s)
                  </span>
                </div>
                <div class="product-row-actions">
                  <span class="badge" class:inactive={!snapshot.price_list.active}>
                    {snapshot.price_list.active ? 'ACTIVA' : 'INACTIVA'}
                  </span>
                  <button
                    class="secondary"
                    type="button"
                    onclick={() => editPriceList(snapshot)}
                  >
                    Editar
                  </button>
                </div>
              </div>
            {:else}
              <p class="muted small">No hay tarifas.</p>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if customersOpen}
    <div class="modal-backdrop" role="presentation">
      <div class="modal customer-modal" role="dialog" aria-modal="true" aria-label="Clientes">
        <div class="modal-heading">
          <div>
            <p class="eyebrow">DATOS MAESTROS</p>
            <h2>Clientes</h2>
          </div>
          <button class="icon" type="button" onclick={() => (customersOpen = false)}>Esc</button>
        </div>

        {#if customerError}
          <p class="error" role="alert">{customerError}</p>
        {/if}

        <form
          class="customer-form"
          onsubmit={(event) => {
            event.preventDefault();
            void saveCustomer();
          }}
        >
          <label class="field">
            <span>Nombre</span>
            <input bind:value={customerFirstName} autocomplete="off" />
          </label>
          <label class="field">
            <span>Primer apellido</span>
            <input bind:value={customerLastName1} autocomplete="off" />
          </label>
          <label class="field">
            <span>Segundo apellido</span>
            <input bind:value={customerLastName2} autocomplete="off" />
          </label>
          <label class="field">
            <span>NIF</span>
            <input bind:value={customerNif} autocomplete="off" />
          </label>
          <label class="field">
            <span>Teléfono</span>
            <input bind:value={customerPhone} inputmode="tel" autocomplete="off" />
          </label>
          <label class="field">
            <span>Email</span>
            <input bind:value={customerEmail} type="email" autocomplete="off" />
          </label>
          <div class="field">
            <span>Estado</span>
            <label class="inline">
              <input type="checkbox" bind:checked={customerActive} />
              Activo
            </label>
          </div>
          <div class="product-actions customer-actions">
            <button class="primary" type="submit" disabled={customerBusy}>
              {customerBusy ? 'Guardando…' : 'Guardar cliente'}
            </button>
            <button class="secondary" type="button" onclick={resetCustomerForm}>
              Nuevo cliente
            </button>
          </div>
        </form>

        <div class="customer-list">
          {#each customers as customer (customer.id)}
            <div class="customer-row">
              <div>
                <strong>{customerLabel(customer)}</strong>
                <span class="muted small">
                  {customer.nif ?? 'sin NIF'} · {customer.phone1 ?? 'sin teléfono'} · {customer.email ?? 'sin email'}
                </span>
              </div>
              <div class="product-row-actions">
                <span class="badge" class:inactive={!customer.active}>
                  {customer.active ? 'ACTIVO' : 'INACTIVO'}
                </span>
                <button class="secondary" type="button" onclick={() => editCustomer(customer)}>
                  Editar
                </button>
              </div>
            </div>
          {:else}
            <p class="muted small">No hay clientes.</p>
          {/each}
        </div>
      </div>
    </div>
  {/if}

  {#if refundOpen && refundInvoice}
    <div class="modal-backdrop" role="presentation">
      <div
        class="modal refund-modal"
        role="dialog"
        aria-modal="true"
        aria-label="Devolución parcial"
      >
        <div class="modal-heading">
          <div>
            <p class="eyebrow">DEVOLUCIÓN</p>
            <h2>Devolución parcial</h2>
            <p class="muted small">
              Factura {refundInvoice.series}-{String(refundInvoice.number).padStart(6, '0')}
            </p>
          </div>
          <button class="icon" type="button" onclick={() => (refundOpen = false)}>Esc</button>
        </div>

        <div class="refund-lines">
          {#each refundInvoice.lines as line (line.source_line_id)}
            <div class="refund-row">
              <div>
                <strong>{line.description}</strong>
                <span class="muted small">
                  Facturado {line.quantity} · {formatMoney(Math.abs(line.gross))}
                </span>
              </div>
              <label class="field">
                <span>Cantidad a devolver</span>
                <input
                  aria-label={`Cantidad a devolver ${line.description}`}
                  value={refundQuantities[line.source_line_id] ?? '0'}
                  inputmode="decimal"
                  oninput={(event) => {
                    refundQuantities[line.source_line_id] = event.currentTarget.value;
                  }}
                />
              </label>
            </div>
          {/each}
        </div>

        <label class="field">
          <span>Motivo de la devolución</span>
          <input bind:value={refundReason} />
        </label>
        <label class="field">
          <span>Forma de reembolso</span>
          <select bind:value={refundMethod}>
            <option value="cash">Efectivo</option>
            <option value="card">Tarjeta</option>
          </select>
        </label>
        <label class="field">
          <span>Referencia del reembolso</span>
          <input bind:value={refundReference} placeholder="Obligatoria para tarjeta" />
        </label>
        <p class="muted small">
          Importe estimado: <strong>{formatMoney(refundPreview)}</strong>
        </p>
        <div class="modal-actions">
          <button
            class="primary"
            type="button"
            disabled={refundBusy}
            onclick={() => void submitRefund()}
          >
            {refundBusy ? 'Emitiendo…' : 'Emitir devolución'}
          </button>
          <button class="secondary" type="button" onclick={() => (refundOpen = false)}>
            Cancelar
          </button>
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
    flex-wrap: wrap;
    gap: 24px;
    margin-bottom: 18px;
  }

  .topbar > div {
    min-width: 0;
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
    box-shadow: var(--shadow);
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
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
    border-radius: 10px;
    padding: 10px 12px;
  }

  .result > span:first-child {
    flex: 1;
    min-width: 0;
  }

  .result .badge {
    white-space: nowrap;
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
    color: var(--on-accent);
  }

  button.success {
    background: var(--success);
    border-color: transparent;
    color: var(--on-success);
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

  .sale-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .invoice-card {
    border-color: var(--success-border);
  }

  .sync-card.warning {
    border-color: var(--warning-border);
  }

  .badge.warn {
    background: var(--warning-soft);
    color: var(--warning);
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
    color: var(--success);
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
    min-width: 0;
  }

  .field input,
  .field select {
    width: 100%;
    min-width: 0;
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
    background: var(--modal-backdrop);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    overflow-y: auto;
    padding: clamp(12px, 3vw, 24px);
    z-index: 20;
  }

  .modal {
    width: min(560px, 100%);
    max-height: calc(100vh - 48px);
    max-height: calc(100dvh - 48px);
    overflow: auto;
    margin: auto;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 18px;
    padding: 22px;
    box-shadow: var(--modal-shadow);
  }

  .users-modal {
    width: min(980px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .timeclock-modal {
    width: min(720px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .timeclock-current {
    display: grid;
    gap: 8px;
    justify-items: start;
    padding: 16px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .timeclock-current p {
    margin: 0;
  }

  .timeclock-elapsed {
    font-family: 'JetBrains Mono', monospace;
    font-size: 1.5rem;
    font-weight: 700;
  }

  .timeclock-history {
    margin-top: 18px;
  }

  .timeclock-list {
    display: grid;
    gap: 4px;
  }

  .timeclock-row {
    display: grid;
    grid-template-columns: 1fr 1.4fr 1.4fr auto;
    gap: 10px;
    align-items: center;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
    font-size: 0.9rem;
  }

  .timeclock-row:last-child {
    border-bottom: 0;
  }

  .notes-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .note-badge {
    min-width: 18px;
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--danger-text);
    color: var(--on-accent);
    font-size: 0.7rem;
    font-weight: 700;
    text-align: center;
  }

  .notes-modal {
    width: min(760px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .note-compose {
    display: grid;
    gap: 4px;
    padding: 16px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .note-compose .field {
    margin: 6px 0;
  }

  .note-compose textarea {
    width: 100%;
    min-width: 0;
    resize: vertical;
    font: inherit;
  }

  .note-recipients {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin: 8px 0 12px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .note-recipients legend {
    padding: 0 4px;
    color: var(--muted);
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .note-recipient {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 0.9rem;
  }

  .note-list {
    margin-top: 18px;
  }

  .note-card {
    display: grid;
    gap: 6px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
  }

  .note-card:last-child {
    border-bottom: 0;
  }

  .note-card p {
    margin: 0;
  }

  .note-card.unread {
    padding-left: 12px;
    border-left: 3px solid var(--accent);
  }

  .note-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .note-card button {
    justify-self: start;
    padding: 6px 10px;
  }

  .history-modal {
    width: min(960px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .history-filters {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr)) auto;
    gap: 10px;
    align-items: end;
    padding: 14px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 12px;
  }

  .history-filters .field {
    margin: 0;
  }

  .history-filter-actions {
    display: flex;
    gap: 6px;
  }

  .history-summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
    margin-top: 14px;
  }

  .history-summary > div {
    display: grid;
    gap: 4px;
    padding: 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .history-summary strong {
    font-size: 1.15rem;
  }

  .history-list {
    display: grid;
    gap: 2px;
    margin-top: 14px;
  }

  .history-row {
    display: grid;
    grid-template-columns: 70px 1fr 1.4fr auto;
    gap: 10px;
    align-items: center;
    width: 100%;
    padding: 10px 4px;
    color: inherit;
    text-align: left;
    background: transparent;
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
  }

  .history-pager {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 14px;
  }

  .history-detail {
    display: grid;
    gap: 14px;
  }

  .history-detail > .icon {
    justify-self: start;
  }

  .history-detail-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .history-detail-head h3,
  .history-detail-head p {
    margin: 0;
  }

  .history-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 22px;
    font-size: 0.9rem;
  }

  .history-lines {
    width: 100%;
    border-collapse: collapse;
  }

  .history-lines th,
  .history-lines td {
    padding: 8px 6px;
    text-align: left;
    border-bottom: 1px solid var(--line);
  }

  .history-lines th {
    color: var(--muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .history-totals {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }

  .history-totals > div {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .history-payments {
    display: grid;
    gap: 6px;
  }

  .history-payment-row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }

  .history-detail-actions {
    justify-content: flex-start;
  }

  .refund-modal {
    width: min(720px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .refund-lines {
    display: grid;
    gap: 8px;
    margin: 14px 0;
  }

  .refund-row {
    display: grid;
    grid-template-columns: 1fr 190px;
    gap: 12px;
    align-items: end;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }

  .user-create {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
    align-items: end;
    padding-bottom: 14px;
    margin-bottom: 14px;
    border-bottom: 1px solid var(--line);
  }

  .user-create .field {
    margin: 0;
    min-width: 0;
  }

  .user-create button {
    min-height: 40px;
    margin-top: 0;
  }

  .user-create button.full {
    grid-column: 1 / -1;
    justify-self: end;
    width: auto;
    min-width: 180px;
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

  .catalog-tabs {
    display: flex;
    gap: 4px;
    margin: 2px 0 16px;
    border-bottom: 1px solid var(--line);
  }

  .catalog-tabs button {
    background: transparent;
    border: 0;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 8px 12px;
    color: var(--muted);
  }

  .catalog-tabs button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }

  .customer-picker {
    display: grid;
    grid-template-columns: minmax(260px, 380px) 1fr;
    gap: 12px;
    align-items: end;
    margin-bottom: 12px;
  }

  .customer-modal {
    width: min(900px, 100%);
    max-height: 90vh;
    overflow: auto;
  }

  .customer-form {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    align-items: end;
    padding-bottom: 14px;
    margin-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .customer-form .field {
    margin: 0;
    min-width: 0;
  }

  .customer-actions {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .customer-list {
    display: grid;
    gap: 8px;
  }

  .customer-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 10px;
    align-items: center;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
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
    min-width: 0;
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

  .stock-summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 10px;
    margin-bottom: 18px;
  }

  .stock-summary-card {
    display: grid;
    gap: 2px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel);
    color: inherit;
    text-align: left;
  }

  button.stock-summary-card {
    cursor: pointer;
  }

  button.stock-summary-card:hover,
  .stock-summary-card.active {
    border-color: var(--accent);
    background: var(--panel-2);
  }

  .stock-summary-card.active {
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .stock-summary-card strong {
    font-size: 1.35rem;
  }

  .stock-summary-card.low strong {
    color: var(--warning);
  }

  .stock-summary-card.out strong {
    color: var(--danger-text);
  }

  .stock-levels-panel .catalog-toolbar {
    flex-wrap: wrap;
  }

  .stock-levels-panel .catalog-filter {
    flex: 1 1 100%;
  }

  .stock-export {
    margin-left: auto;
    white-space: nowrap;
  }

  .price-list-item-form {
    display: grid;
    grid-template-columns: minmax(200px, 1fr) 120px auto;
    gap: 10px;
    align-items: end;
    flex: 1;
  }

  .price-list-item-form .field {
    margin: 0;
  }

  .stock-layout {
    display: grid;
    grid-template-columns: minmax(280px, 1fr) minmax(320px, 1.15fr);
    gap: 18px;
  }

  .stock-levels-panel,
  .stock-detail-panel {
    min-width: 0;
  }

  .stock-level-list {
    display: grid;
    gap: 6px;
    max-height: 390px;
    overflow: auto;
  }

  .stock-level-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 9px 11px;
    color: inherit;
    font: inherit;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 9px;
    cursor: pointer;
  }

  .stock-level-row.selected {
    border-color: var(--accent);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .stock-level-main,
  .stock-level-side {
    display: grid;
    gap: 2px;
  }

  .stock-level-side {
    justify-items: end;
    white-space: nowrap;
  }

  .stock-detail-panel {
    padding-left: 18px;
    border-left: 1px solid var(--line);
  }

  .stock-detail-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }

  .stock-detail-heading h3 {
    margin: 2px 0;
  }

  .stock-current {
    display: grid;
    justify-items: end;
  }

  .stock-current strong {
    font-size: 1.7rem;
  }

  .stock-form {
    display: grid;
    grid-template-columns: minmax(120px, 0.9fr) 90px minmax(140px, 1.4fr) auto;
    gap: 8px;
    align-items: end;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--line);
  }

  .stock-form .field {
    margin: 0;
    min-width: 0;
  }

  .stock-form .product-actions {
    grid-column: auto;
  }

  .stock-min-form {
    display: flex;
    align-items: end;
    gap: 8px;
    padding: 10px 0;
    border-bottom: 1px solid var(--line);
  }

  .stock-min-form .field {
    margin: 0;
    width: 140px;
  }

  .stock-count-title {
    margin: 14px 0 6px;
  }

  .stock-count-form {
    display: grid;
    grid-template-columns: 120px minmax(160px, 1fr) auto;
    gap: 8px;
    align-items: end;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel-2);
  }

  .stock-count-form .field {
    margin: 0;
    min-width: 0;
  }

  .stock-count-preview {
    margin: 6px 0 12px;
  }

  .stock-count-preview .negative {
    color: var(--danger-text);
  }

  .stock-movements {
    margin-top: 12px;
  }

  .stock-movements-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .stock-history-export {
    padding: 4px 10px;
    font-size: 0.78rem;
  }

  .stock-movement-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line);
  }

  .stock-movement-row > span {
    display: grid;
    gap: 2px;
  }

  .stock-level-row .negative,
  .stock-current .negative,
  .stock-movement-row .negative {
    color: var(--danger-text);
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
    background: var(--danger-soft);
    color: var(--danger-text);
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

  .import-panel {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }

  .import-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    align-items: end;
    margin: 10px 0;
  }

  .import-grid .field {
    margin: 0;
    min-width: 0;
  }

  .import-grid input[type='file'] {
    padding: 6px;
    font-size: 0.82rem;
  }

  .import-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .import-report {
    margin-top: 10px;
    padding: 10px 12px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .import-summary {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }

  .import-errors {
    margin: 8px 0 0;
    padding-left: 20px;
    color: var(--danger-text);
    font-size: 0.82rem;
  }

  .badge.ok {
    background: var(--success-soft);
    color: var(--success);
    border-color: var(--success-border);
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
    color: var(--on-accent);
    border-color: transparent;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
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
    position: relative;
  }

  .theme-button {
    position: absolute;
    top: 14px;
    right: 14px;
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

  @media (max-width: 1200px) {
    .topbar {
      flex-direction: column;
      align-items: stretch;
      gap: 12px;
    }

    .topbar-meta {
      justify-content: flex-start;
      gap: 6px;
    }

    .topbar-meta button {
      padding: 7px 10px;
    }
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

    .stock-layout {
      grid-template-columns: 1fr;
    }

    .stock-detail-panel {
      padding-top: 14px;
      padding-left: 0;
      border-top: 1px solid var(--line);
      border-left: 0;
    }
  }

  @media (max-width: 760px) {
    .modal {
      padding: 18px;
      border-radius: 14px;
    }

    .user-create,
    .customer-form,
    .product-form,
    .stock-form,
    .user-fields {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .product-actions,
    .stock-form .product-actions {
      grid-column: 1 / -1;
    }

    .import-grid {
      grid-template-columns: 1fr;
    }

    .price-list-item-form {
      grid-template-columns: 1fr;
    }

    .timeclock-row {
      grid-template-columns: 1fr;
      gap: 4px;
    }

    .history-filters {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .history-filter-actions {
      grid-column: 1 / -1;
    }

    .history-summary,
    .history-totals {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .history-row {
      grid-template-columns: 58px 1fr auto;
    }

    .history-row .muted {
      grid-column: 1 / -1;
    }

    .history-detail-head {
      flex-direction: column;
    }

    .customer-picker,
    .refund-row,
    .customer-row {
      grid-template-columns: 1fr;
    }

    .customer-row {
      align-items: stretch;
    }

    .modal-actions button {
      flex: 1;
    }
  }

  @media (max-width: 520px) {
    .user-create,
    .customer-form,
    .product-form,
    .stock-form,
    .stock-count-form,
    .user-fields {
      grid-template-columns: 1fr;
    }
  }
</style>
