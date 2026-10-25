import { expect, test } from '@playwright/test';

test.describe('TPV desktop UI', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      const storeId = '00000000-0000-7000-8000-000000000001';
      const registerId = '00000000-0000-7000-8000-000000000002';
      const sellerId = '00000000-0000-7000-8000-000000000003';
      const customerId = '00000000-0000-7000-8000-000000000004';
      const userId = '00000000-0000-7000-8000-000000000005';
      const now = () => new Date().toISOString();

      const customerRecords: any[] = [
        {
          id: customerId,
          first_name: 'Cliente',
          last_name1: 'Contado',
          last_name2: null,
          phone1: null,
          email: null,
          nif: '12345678Z',
          address: { street: '', number: '', locality: '', postal_code: '', province: '' },
          active: true
        }
      ];

      let session: Record<string, unknown> | null = null;
      let sale: Record<string, unknown> | null = null;
      let totals = {
        gross: 0,
        net: 0,
        tax: 0,
        tax_breakdown: [],
        paid: 0,
        received: 0,
        due: 0,
        change: 0
      };

      const saleView = () => ({ sale, totals });
      const emptyTotals = () => ({
        gross: 0,
        net: 0,
        tax: 0,
        tax_breakdown: [],
        paid: 0,
        received: 0,
        due: 0,
        change: 0
      });

      const user = () => ({
        id: userId,
        username: 'admin',
        display_name: 'Administrador',
        role: 'admin',
        store_ids: [storeId],
        active: true
      });

      const catalogItems: Array<Record<string, unknown>> = [];
      const sellerItems: Array<Record<string, unknown>> = [];
      const storeItems: Array<Record<string, unknown>> = [
        {
          id: storeId,
          code: 'BEN01',
          name: 'Caja 1',
          tax_id: 'B12345674',
          phone: null,
          address: { street: '', number: '', locality: '', postal_code: '', province: '' },
          active: true
        }
      ];
      const familyItems: Array<Record<string, unknown>> = [
        {
          id: '00000000-0000-7000-8000-000000000010',
          code: 'GENERAL',
          name: 'General',
          active: true
        }
      ];
      const taxItems: Array<Record<string, unknown>> = [
        {
          id: '00000000-0000-7000-8000-000000000011',
          code: 'IVA21',
          name: 'IVA general 21%',
          percent_bp: 2100,
          active: true
        }
      ];
      const priceListItems: Array<Record<string, unknown>> = [
        {
          price_list: {
            id: '00000000-0000-7000-8000-000000000012',
            code: 'EMPLEADOS',
            name: 'Tarifa empleados',
            active: true
          },
          items: []
        }
      ];
      const stockMovements: Array<Record<string, any>> = [];
      const stockLevels = new Map<string, Record<string, any>>();
      const stockLevelFor = (product: Record<string, any>) => {
        const cached = stockLevels.get(String(product.id));
        if (cached) {
          cached.sku = product.sku;
          cached.name = product.name;
          return cached;
        }
        const level = {
          store_id: storeId,
          product_id: product.id,
          sku: product.sku,
          name: product.name,
          quantity: '0',
          min_quantity: '0',
          updated_at: null
        };
        stockLevels.set(String(product.id), level);
        return level;
      };
      let cashSession: any = null;
      const timeEntries: any[] = [];
      const timeClockStatus = () => {
        const active = timeEntries.find((entry) => !entry.clock_out) ?? null;
        return {
          active: active ? { entry: active, user_name: 'Administrador' } : null,
          recent: [...timeEntries]
            .reverse()
            .map((entry) => ({ entry, user_name: 'Administrador' }))
        };
      };
      const noteViews: any[] = [];
      const noteInbox = () => ({
        notes: [...noteViews].reverse(),
        unread: noteViews.filter((view) => !view.is_author && !view.viewer_read_at).length,
        recipients: [{ user_id: userId, user_name: 'Administrador' }]
      });
      let invoiceRecord: any = null;
      const previewStockCountImport = (csv: string, commit: boolean) => {
        (window as any).__lastCountImport = { csv, commit };
        const normalize = (value: string) =>
          value
            .trim()
            .toLowerCase()
            .normalize('NFD')
            .replace(/[\u0300-\u036f]/g, '')
            .replace(/[^a-z0-9]+/g, '_')
            .replace(/^_|_$/g, '');
        const lines = csv.replace(/\r\n/g, '\n').split('\n').filter((line) => line.trim());
        if (lines.length < 2) throw 'El fichero no contiene filas.';
        const delimiter = lines[0].includes(';') ? ';' : ',';
        const headers = lines[0].split(delimiter).map(normalize);
        const skuIndex = headers.findIndex((header) =>
          ['codigo', 'sku', 'codigo_articulo', 'referencia', 'ref', 'articulo_id'].includes(header)
        );
        const countIndex = headers.findIndex((header) =>
          ['conteo_fisico', 'conteo', 'cantidad_contada', 'stock_contado', 'contado'].includes(header)
        );
        const reasonIndex = headers.findIndex((header) =>
          ['motivo', 'motivo_conteo', 'observaciones', 'comentario'].includes(header)
        );
        if (skuIndex < 0 || countIndex < 0) throw 'Faltan columnas obligatorias en el CSV.';
        const errors: Array<{ line: number; sku: string | null; message: string }> = [];
        const rows: Array<{ product: Record<string, any>; counted: number; reason: string }> = [];
        const seen = new Set<string>();
        let skipped = 0;
        for (let index = 1; index < lines.length; index += 1) {
          const cells = lines[index].split(delimiter);
          const sku = String(cells[skuIndex] ?? '').trim();
          const rawCount = String(cells[countIndex] ?? '').trim();
          const line = index + 1;
          if (!rawCount) {
            skipped += 1;
            continue;
          }
          if (!sku) {
            errors.push({ line, sku: null, message: 'El código es obligatorio.' });
            continue;
          }
          if (seen.has(sku)) {
            errors.push({ line, sku, message: 'El código está repetido.' });
            continue;
          }
          seen.add(sku);
          const product = catalogItems.find((item) => String(item.sku) === sku);
          if (!product) {
            errors.push({ line, sku, message: 'SKU no encontrado en el catálogo' });
            continue;
          }
          const normalizedCount = rawCount.replace(',', '.');
          if ((normalizedCount.split('.')[1] ?? '').length > 3) {
            errors.push({ line, sku, message: 'El conteo admite como máximo tres decimales.' });
            continue;
          }
          const counted = Number(normalizedCount);
          if (!Number.isFinite(counted) || counted < 0) {
            errors.push({ line, sku, message: 'El conteo no puede ser negativo.' });
            continue;
          }
          rows.push({
            product,
            counted,
            reason: String(cells[reasonIndex] ?? '').trim() || 'Conteo físico importado'
          });
        }
        const adjust = rows.filter(
          (row) => Number(stockLevelFor(row.product).quantity) !== row.counted
        ).length;
        const report = {
          total_rows: lines.length - 1,
          skipped_rows: skipped,
          valid_rows: rows.length,
          invalid_rows: errors.length,
          adjusted: adjust,
          unchanged: rows.length - adjust,
          committed: false,
          errors
        };
        if (commit && errors.length === 0 && rows.length > 0) {
          for (const row of rows) {
            const level = stockLevelFor(row.product);
            const previous = Number(level.quantity);
            const difference = Number((row.counted - previous).toFixed(3));
            if (difference !== 0) {
              stockMovements.unshift({
                id: crypto.randomUUID(),
                store_id: storeId,
                product_id: row.product.id,
                kind: difference > 0 ? 'adjustment_in' : 'adjustment_out',
                quantity: String(Math.abs(difference)),
                reason: row.reason,
                sale_id: null,
                sale_line_id: null,
                occurred_at: now()
              });
            }
            level.quantity = String(row.counted);
            level.updated_at = now();
          }
          report.committed = true;
        }
        return report;
      };
      const api = {
        async current_user() {
          return session;
        },
        async login(username: string, password: string) {
          if (username !== 'admin' || password !== 'admin123') {
            throw 'credenciales inválidas';
          }
          session = { session_id: crypto.randomUUID(), user: user(), started_at: now() };
          return session;
        },
        async logout() {
          session = null;
        },
        async bootstrap() {
          return {
            store_name: '100%SHOPPING',
            store_id: storeId,
            register_id: registerId,
            seller_id: sellerId,
            customer_id: customerId,
            user_id: userId,
            products: [
              {
                id: '00000000-0000-7000-8000-000000000010',
                sku: '3',
                barcode: null,
                name: 'Café solo',
                family_code: null,
                unit_price: '1.20',
                tax_rate: { code: 'IVA10', percent_bp: 1000 },
                active: true
              }
            ],
            open_cash: null
          };
        },
        async open_cash() {
          cashSession = {
            id: crypto.randomUUID(),
            store_id: storeId,
            register_id: registerId,
            opened_by: userId,
            opened_at: now(),
            opening_float: 10000,
            movements: [],
            close_history: [],
            reopen_history: [],
            state: 'open',
            version: 1
          };
          return cashSession;
        },
        async cash_status() {
          return cashSession;
        },
        async cash_movement(
          sessionId: string,
          kind: string,
          amountCents: number,
          reason: string
        ) {
          if (!cashSession || cashSession.id !== sessionId) throw 'caja no abierta';
          cashSession.movements.push({
            id: crypto.randomUUID(),
            kind,
            amount: amountCents,
            reason,
            created_at: now()
          });
          return cashSession;
        },
        async time_clock_status() {
          return timeClockStatus();
        },
        async time_clock_in() {
          if (timeEntries.some((entry) => !entry.clock_out)) {
            throw 'Ya existe un fichaje abierto.';
          }
          timeEntries.push({
            id: crypto.randomUUID(),
            user_id: userId,
            store_id: storeId,
            clock_in: now(),
            clock_out: null,
            correction_reason: null
          });
          return timeClockStatus();
        },
        async time_clock_out() {
          const active = timeEntries.find((entry) => !entry.clock_out);
          if (!active) throw 'No hay ningún fichaje abierto.';
          active.clock_out = now();
          return timeClockStatus();
        },
        async note_list() {
          return noteInbox();
        },
        async note_create(request: any) {
          const note = {
            id: crypto.randomUUID(),
            store_id: storeId,
            author_id: userId,
            title: request.title,
            body: request.body,
            created_at: now()
          };
          noteViews.push({
            note,
            author_name: 'Administrador',
            recipients: (request.recipient_ids as string[]).map((id) => ({
              user_id: id,
              user_name: 'Administrador',
              read_at: null
            })),
            viewer_read_at: null,
            is_author: true
          });
          return noteInbox();
        },
        async note_read(noteId: string) {
          const view = noteViews.find((item) => item.note.id === noteId);
          if (!view) throw 'Nota no encontrada.';
          view.viewer_read_at = now();
          return noteInbox();
        },
        async list_sales() {
          return sale && sale.ticket_number ? [saleView()] : [];
        },
        async query_sales(request: any) {
          const hasSale = Boolean(sale && sale.ticket_number);
          const ticket = request.ticket_number;
          const matches =
            hasSale && (ticket == null || Number(ticket) === Number(sale?.ticket_number));
          const items = matches ? [saleView()] : [];
          return {
            items,
            total: items.length,
            limit: request.limit ?? 25,
            offset: request.offset ?? 0,
            gross: items.length ? totals.gross : 0,
            tax: items.length ? totals.tax : 0,
            paid: items.length ? totals.paid : 0
          };
        },
        async sync_status() {
          return {
            pending: 0,
            sent: 0,
            failed: 0,
            oldest_pending_at: null,
            last_sent_at: null,
            last_failed_at: null,
            last_error: null
          };
        },
        async list_failed_sync_events() {
          return [];
        },
        async search_products(query: string) {
          if (!query.toLowerCase().includes('caf')) {
            return [];
          }
          return [
            {
              id: '00000000-0000-7000-8000-000000000010',
              sku: '3',
              name: 'Café solo',
              price_cents: 120,
              tax_code: 'IVA10',
              tax_percent_bp: 1000
            }
          ];
        },
        async list_stock_levels_for_products(productIds: string[]) {
          return productIds.flatMap((productId) => {
            if (productId === '00000000-0000-7000-8000-000000000010') {
              return [
                {
                  store_id: storeId,
                  product_id: productId,
                  sku: '3',
                  name: 'Café solo',
                  quantity: '5',
                  min_quantity: '10',
                  updated_at: null
                }
              ];
            }
            const product = catalogItems.find((item) => item.id === productId);
            return product ? [{ ...stockLevelFor(product) }] : [];
          });
        },
        async list_customers() {
          return customerRecords;
        },
        async list_users() {
          return [user()];
        },
        async list_stores() {
          return storeItems;
        },
        async list_price_lists() {
          return priceListItems;
        },
        async save_price_list(request: any) {
          const existing = request.price_list_id
            ? priceListItems.find((item) => item.price_list.id === request.price_list_id)
            : null;
          const snapshot = {
            price_list: {
              id: existing?.price_list.id ?? crypto.randomUUID(),
              code: String(request.code).toUpperCase(),
              name: request.name,
              active: request.active
            },
            items: request.items ?? []
          };
          if (existing) {
            Object.assign(existing, snapshot);
          } else {
            priceListItems.push(snapshot);
          }
          return snapshot;
        },
        async list_stock_levels() {
          return catalogItems.map((product) => ({ ...stockLevelFor(product) }));
        },
        async export_stock_csv(request: any) {
          (window as any).__lastStockExportRequest = request;
          return '/tmp/stock-e2e.csv';
        },
        async export_stock_replenishment_csv(request: any) {
          (window as any).__lastReplenishmentRequest = request;
          return '/tmp/pedido-reposicion-e2e.csv';
        },
        async export_stock_count_sheet_csv(request: any) {
          (window as any).__lastCountSheetRequest = request;
          return '/tmp/conteo-fisico-e2e.csv';
        },
        async export_stock_movements_csv(_productId: string) {
          return '/tmp/movimientos-e2e.csv';
        },
        async export_stock_ledger_csv(_request: any) {
          return '/tmp/movimientos-stock-e2e.csv';
        },
        async count_low_stock() {
          return catalogItems.reduce((count, product) => {
            const level = stockLevelFor(product);
            const minimum = Number.parseFloat(level.min_quantity);
            const low =
              Number.isFinite(minimum) &&
              minimum > 0 &&
              Number.parseFloat(level.quantity) <= minimum;
            return count + (low ? 1 : 0);
          }, 0);
        },
        async list_stock_movements(productId: string, limit = 100) {
          return stockMovements
            .filter((movement) => movement.product_id === productId)
            .slice(0, limit);
        },
        async query_stock_movements(request: any) {
          (window as any).__lastLedgerRequest = request;
          const search = String(request.search ?? '').trim().toLowerCase();
          const productId = request.product_id ?? null;
          const kind = request.kind ?? null;
          const from = request.from_ms == null ? null : Number(request.from_ms);
          const to = request.to_ms == null ? null : Number(request.to_ms);
          const matches = stockMovements
            .filter((movement) => !productId || movement.product_id === productId)
            .filter((movement) => !kind || movement.kind === kind)
            .filter((movement) => {
              const occurred = Date.parse(movement.occurred_at);
              if (from != null && occurred < from) return false;
              if (to != null && occurred > to) return false;
              if (!search) return true;
              const product = catalogItems.find((item) => item.id === movement.product_id);
              return (
                String(product?.sku ?? '').toLowerCase().includes(search) ||
                String(product?.name ?? '').toLowerCase().includes(search) ||
                String(movement.reason ?? '').toLowerCase().includes(search)
              );
            })
            .map((movement) => {
              const product = catalogItems.find((item) => item.id === movement.product_id);
              return {
                ...movement,
                sku: product?.sku ?? '',
                name: product?.name ?? ''
              };
            });
          const limit = Number(request.limit ?? 100);
          const offset = Number(request.offset ?? 0);
          return {
            items: matches.slice(offset, offset + limit),
            total: matches.length,
            limit,
            offset
          };
        },
        async record_stock_movement(request: any) {
          const product = catalogItems.find((item) => item.id === request.product_id);
          if (!product) throw 'Producto no encontrado.';
          const level = stockLevelFor(product);
          const quantity = Number.parseFloat(String(request.quantity).replace(',', '.'));
          if (!Number.isFinite(quantity) || quantity <= 0) {
            throw 'La cantidad debe ser mayor que cero.';
          }
          const delta = request.kind === 'adjustment_out' ? -quantity : quantity;
          level.quantity = String(Number((Number(level.quantity) + delta).toFixed(3)));
          level.updated_at = now();
          const movement = {
            id: crypto.randomUUID(),
            store_id: storeId,
            product_id: request.product_id,
            kind: request.kind,
            quantity: String(quantity),
            reason: request.reason ?? null,
            sale_id: null,
            sale_line_id: null,
            occurred_at: now()
          };
          stockMovements.unshift(movement);
          return movement;
        },
        async set_stock_minimum(request: any) {
          const product = catalogItems.find((item) => item.id === request.product_id);
          if (!product) throw 'Producto no encontrado.';
          const minimum = Number.parseFloat(String(request.min_quantity).replace(',', '.'));
          if (!Number.isFinite(minimum) || minimum < 0) {
            throw 'El mínimo no puede ser negativo.';
          }
          const level = stockLevelFor(product);
          level.min_quantity = String(minimum);
          level.updated_at = now();
          return { ...level };
        },
        async set_stock_count(request: any) {
          const product = catalogItems.find((item) => item.id === request.product_id);
          if (!product) throw 'Producto no encontrado.';
          const counted = Number.parseFloat(
            String(request.counted_quantity).replace(',', '.')
          );
          if (!Number.isFinite(counted) || counted < 0) {
            throw 'El conteo no puede ser negativo.';
          }
          const reason = String(request.reason ?? '').trim();
          if (!reason) throw 'El motivo del conteo es obligatorio.';
          const level = stockLevelFor(product);
          const previous = Number(level.quantity);
          const difference = Number((counted - previous).toFixed(3));
          let movement = null;
          if (difference !== 0) {
            movement = {
              id: crypto.randomUUID(),
              store_id: storeId,
              product_id: request.product_id,
              kind: difference > 0 ? 'adjustment_in' : 'adjustment_out',
              quantity: String(Math.abs(difference)),
              reason,
              sale_id: null,
              sale_line_id: null,
              occurred_at: now()
            };
            stockMovements.unshift(movement);
          }
          level.quantity = String(counted);
          level.updated_at = now();
          return {
            level: { ...level },
            previous_quantity: String(previous),
            counted_quantity: String(counted),
            difference: String(difference),
            movement
          };
        },
        async transfer_stock(request: any) {
          (window as any).__lastTransferRequest = request;
          const product = catalogItems.find((item) => item.id === request.product_id);
          if (!product) throw 'Producto no encontrado.';
          if (!request.to_store_id || request.to_store_id === storeId) {
            throw 'La tienda de destino debe ser distinta de la tienda de origen.';
          }
          const quantity = Number.parseFloat(String(request.quantity).replace(',', '.'));
          if (!Number.isFinite(quantity) || quantity <= 0) {
            throw 'La cantidad debe ser mayor que cero.';
          }
          const level = stockLevelFor(product);
          if (Number(level.quantity) < quantity) throw 'stock insuficiente';
          level.quantity = String(Number((Number(level.quantity) - quantity).toFixed(3)));
          level.updated_at = now();
          const movement = {
            id: crypto.randomUUID(),
            store_id: storeId,
            product_id: request.product_id,
            kind: 'transfer_out',
            quantity: String(quantity),
            reason: `Transferencia a ${request.to_store_id}`,
            sale_id: null,
            sale_line_id: null,
            occurred_at: now()
          };
          stockMovements.unshift(movement);
          return {
            transfer: {
              id: crypto.randomUUID(),
              from_store_id: storeId,
              to_store_id: request.to_store_id,
              product_id: request.product_id,
              sku: product.sku,
              name: product.name,
              quantity: String(quantity),
              reason: request.reason ?? null,
              occurred_at: now()
            },
            source_level: { ...level },
            destination_level: {
              ...level,
              store_id: request.to_store_id,
              quantity: String(quantity)
            }
          };
        },
        async preview_stock_count_import(csv: string) {
          return previewStockCountImport(csv, false);
        },
        async commit_stock_count_import(csv: string) {
          return previewStockCountImport(csv, true);
        },
        async save_customer(request: any) {
          const existing = request.customer_id
            ? customerRecords.find((customer) => customer.id === request.customer_id)
            : null;
          const record = {
            id: existing?.id ?? crypto.randomUUID(),
            first_name: request.first_name,
            last_name1: request.last_name1,
            last_name2: request.last_name2 ?? null,
            phone1: request.phone ?? null,
            email: request.email ?? null,
            nif: request.nif ?? null,
            address: existing?.address ?? {
              street: '',
              number: '',
              locality: '',
              postal_code: '',
              province: ''
            },
            active: request.active
          };
          if (existing) {
            Object.assign(existing, record);
          } else {
            customerRecords.push(record);
          }
          return record;
        },
        async start_sale(customerIdArg?: string, priceListIdArg?: string) {
          sale = {
            id: crypto.randomUUID(),
            store_id: storeId,
            register_id: registerId,
            seller_id: sellerId,
            customer_id: customerIdArg ?? customerId,
            price_list_id: priceListIdArg ?? null,
            lines: [],
            payments: [],
            state: 'draft',
            created_at: now(),
            closed_at: null,
            invoice_id: null,
            ticket_series: null,
            ticket_number: null,
            notes: null
          };
          totals = emptyTotals();
          return saleView();
        },
        async add_line(_saleId: string, sku: string) {
          if (!sale) throw 'venta no iniciada';
          sale.lines = [
            {
              id: crypto.randomUUID(),
              product_id: '00000000-0000-7000-8000-000000000010',
              sku,
              description: 'Café solo',
              unit_price: '1.20',
              quantity: '1',
              discount_bp: 0,
              tax_rate: { code: 'IVA10', percent_bp: 1000 },
              kind: 'sale'
            }
          ];
          sale.state = 'awaiting_payment';
          totals = {
            gross: 120,
            net: 109,
            tax: 11,
            tax_breakdown: [
              { code: 'IVA10', percent_bp: 1000, gross: 120, net: 109, tax: 11 }
            ],
            paid: 0,
            received: 0,
            due: 120,
            change: 0
          };
          return saleView();
        },
        async apply_payment(
          _saleId: string,
          method: string,
          amountCents: number,
          receivedCents: number
        ) {
          if (!sale) throw 'venta no iniciada';
          sale.payments = [
            {
              id: crypto.randomUUID(),
              method,
              amount: amountCents,
              received: receivedCents,
              reference: null
            }
          ];
          totals = {
            ...totals,
            paid: amountCents,
            received: receivedCents,
            due: 0,
            change: Math.max(0, receivedCents - amountCents)
          };
          return saleView();
        },
        async confirm_sale() {
          if (!sale) throw 'venta no iniciada';
          sale.state = 'paid';
          sale.ticket_number = 1;
          sale.ticket_series = 'TPV';
          sale.closed_at = now();
          return saleView();
        },
        async issue_invoice() {
          if (!sale) throw 'venta no iniciada';
          sale.invoice_id = '00000000-0000-7000-8000-000000000901';
          const source = sale.lines[0];
          invoiceRecord = {
            id: sale.invoice_id,
            sale_id: sale.id,
            customer_id: sale.customer_id ?? customerId,
            series: 'FAC',
            number: 1,
            issued_at: now(),
            kind: { type: 'standard' },
            lines: [
              {
                source_line_id: source.id,
                description: source.description,
                quantity: source.quantity,
                unit_price: source.unit_price,
                discount_bp: 0,
                tax_rate: source.tax_rate,
                gross: totals.gross,
                tax: totals.tax,
                kind: 'sale'
              }
            ],
            totals
          };
          return invoiceRecord;
        },
        async get_invoice(invoiceId: string) {
          if (!invoiceRecord || invoiceRecord.id !== invoiceId) throw 'factura no encontrada';
          return invoiceRecord;
        },
        async rectify_invoice(invoiceId: string, reason: string) {
          return {
            id: crypto.randomUUID(),
            sale_id: sale?.id ?? '',
            customer_id: sale?.customer_id ?? customerId,
            series: 'REC',
            number: 1,
            issued_at: now(),
            kind: {
              type: 'rectificative',
              data: { original_invoice_id: invoiceId, reason }
            },
            lines: [],
            totals
          };
        },
        async refund_invoice_lines(
          invoiceId: string,
          reason: string,
          refundMethod: string,
          _refundReference: string | null,
          lines: Array<{ source_line_id: string; quantity: string }>
        ) {
          if (!invoiceRecord || invoiceRecord.id !== invoiceId) throw 'factura no encontrada';
          let amount = 0;
          for (const selection of lines) {
            const line = invoiceRecord.lines.find(
              (entry: any) => entry.source_line_id === selection.source_line_id
            );
            if (!line) throw 'línea no encontrada';
            amount += Math.round(
              (Math.abs(line.gross) * Number(selection.quantity)) / Number(line.quantity)
            );
          }
          if (refundMethod === 'cash' && cashSession) {
            cashSession.movements.push({
              id: crypto.randomUUID(),
              kind: 'expense',
              amount,
              reason: 'Devolución REC-000001',
              created_at: now()
            });
          }
          return {
            invoice: {
              id: crypto.randomUUID(),
              sale_id: invoiceRecord.sale_id,
              customer_id: invoiceRecord.customer_id,
              series: 'REC',
              number: 1,
              issued_at: now(),
              kind: {
                type: 'rectificative',
                data: { original_invoice_id: invoiceId, reason }
              },
              lines: [],
              totals: {
                ...totals,
                gross: -amount,
                net: -amount,
                tax: 0,
                tax_breakdown: [],
                paid: 0,
                received: 0,
                due: 0,
                change: 0
              }
            },
            cash: refundMethod === 'cash' ? cashSession : null
          };
        },
        async print_ticket() {
          return { job_id: crypto.randomUUID(), file: 'null', bytes: 1045, success: true };
        }
      };

      (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
          switch (cmd) {
            case 'current_user':
              return api.current_user();
            case 'login':
              return api.login(String(args.username), String(args.password));
            case 'logout':
              return api.logout();
            case 'bootstrap':
              return api.bootstrap();
            case 'list_customers':
              return api.list_customers();
            case 'list_users':
              return api.list_users();
            case 'list_stores':
              return api.list_stores();
            case 'save_customer':
              return api.save_customer(args.request);
            case 'open_cash':
              return api.open_cash();
            case 'cash_status':
              return api.cash_status();
            case 'cash_movement':
              return api.cash_movement(
                String(args.sessionId),
                String(args.kind),
                Number(args.amountCents),
                String(args.reason)
              );
            case 'list_sales':
              return api.list_sales();
            case 'query_sales':
              return api.query_sales(args.request);
            case 'sync_status':
              return api.sync_status();
            case 'sync_now':
              return {
                accepted: 1,
                duplicates: 0,
                rejected: 0,
                pulled: 1,
                users_applied: 1,
                master_applied: 0,
                master_conflicts: 0
              };
            case 'list_failed_sync_events':
              return api.list_failed_sync_events();
            case 'time_clock_status':
              return api.time_clock_status();
            case 'time_clock_in':
              return api.time_clock_in();
            case 'time_clock_out':
              return api.time_clock_out();
            case 'note_list':
              return api.note_list();
            case 'note_create':
              return api.note_create(args.request);
            case 'note_read':
              return api.note_read(String(args.noteId));
            case 'search_products':
              return api.search_products(String(args.query));
            case 'list_products':
              return catalogItems;
            case 'list_families':
              return familyItems;
            case 'list_taxes':
              return taxItems;
            case 'list_sellers':
              return sellerItems;
            case 'list_price_lists':
              return api.list_price_lists();
            case 'save_price_list':
              return api.save_price_list(args.request);
            case 'list_stock_levels':
              return api.list_stock_levels();
            case 'export_stock_csv':
              return api.export_stock_csv(args.request);
            case 'export_stock_replenishment_csv':
              return api.export_stock_replenishment_csv(args.request);
            case 'export_stock_count_sheet_csv':
              return api.export_stock_count_sheet_csv(args.request);
            case 'export_stock_movements_csv':
              return api.export_stock_movements_csv(String(args.productId));
            case 'list_stock_levels_for_products':
              return api.list_stock_levels_for_products(args.productIds as string[]);
            case 'count_low_stock':
              return api.count_low_stock();
            case 'list_stock_movements':
              return api.list_stock_movements(String(args.productId), Number(args.limit ?? 100));
            case 'query_stock_movements':
              return api.query_stock_movements(args.request);
            case 'record_stock_movement':
              return api.record_stock_movement(args.request);
            case 'set_stock_minimum':
              return api.set_stock_minimum(args.request);
            case 'set_stock_count':
              return api.set_stock_count(args.request);
            case 'transfer_stock':
              return api.transfer_stock(args.request);
            case 'preview_stock_count_import':
              return api.preview_stock_count_import(String(args.csv));
            case 'commit_stock_count_import':
              return api.commit_stock_count_import(String(args.csv));
            case 'export_stock_ledger_csv':
              return api.export_stock_ledger_csv(args.request);
            case 'list_master_data_conflicts':
              return [];
            case 'save_product': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.product_id ?? crypto.randomUUID(),
                sku: request.sku,
                barcode: request.barcode ?? null,
                name: request.name,
                family_code: request.family_code ?? null,
                unit_price: request.unit_price,
                tax_rate: {
                  code: request.tax_code,
                  percent_bp: request.tax_percent_bp
                },
                active: request.active
              };
              const index = catalogItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                catalogItems[index] = saved;
              } else {
                catalogItems.push(saved);
              }
              return saved;
            }
            case 'save_family': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.family_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                active: request.active
              };
              const index = familyItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                familyItems[index] = saved;
              } else {
                familyItems.push(saved);
              }
              return saved;
            }
            case 'save_tax': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.tax_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                percent_bp: request.percent_bp,
                active: request.active
              };
              const index = taxItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                taxItems[index] = saved;
              } else {
                taxItems.push(saved);
              }
              return saved;
            }
            case 'save_seller': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.seller_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                phone1: request.phone1 ?? null,
                phone2: request.phone2 ?? null,
                active: request.active
              };
              const index = sellerItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                sellerItems[index] = saved;
              } else {
                sellerItems.push(saved);
              }
              return saved;
            }
            case 'save_store': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.store_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                tax_id: String(request.tax_id).toUpperCase(),
                phone: request.phone ?? null,
                address: {
                  street: request.street ?? '',
                  number: request.number ?? '',
                  locality: request.locality ?? '',
                  postal_code: request.postal_code ?? '',
                  province: request.province ?? ''
                },
                active: request.active
              };
              const index = storeItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                storeItems[index] = saved;
              } else {
                storeItems.push(saved);
              }
              return saved;
            }
            case 'preview_product_import':
              return {
                source_system: String(args.sourceSystem),
                batch_id: 'e2e-batch',
                dry_run: true,
                committed: false,
                already_imported: false,
                total_rows: 2,
                valid_rows: 2,
                invalid_rows: 0,
                inserted: 1,
                updated: 1,
                unchanged: 0,
                errors: []
              };
            case 'commit_product_import':
              return {
                source_system: String(args.sourceSystem),
                batch_id: 'e2e-batch',
                dry_run: false,
                committed: true,
                already_imported: false,
                total_rows: 2,
                valid_rows: 2,
                invalid_rows: 0,
                inserted: 1,
                updated: 1,
                unchanged: 0,
                errors: []
              };
            case 'start_sale':
              return api.start_sale(
                args.customerId ? String(args.customerId) : undefined,
                args.priceListId ? String(args.priceListId) : undefined
              );
            case 'add_line':
              return api.add_line(String(args.saleId), String(args.sku));
            case 'apply_payment':
              return api.apply_payment(
                String(args.saleId),
                String(args.method),
                Number(args.amountCents),
                Number(args.receivedCents)
              );
            case 'confirm_sale':
              return api.confirm_sale();
            case 'issue_invoice':
              return api.issue_invoice();
            case 'rectify_invoice':
              return api.rectify_invoice(String(args.invoiceId), String(args.reason));
            case 'get_invoice':
              return api.get_invoice(String(args.invoiceId));
            case 'refund_invoice_lines':
              return api.refund_invoice_lines(
                String(args.invoiceId),
                String(args.reason),
                String(args.refundMethod),
                args.refundReference ? String(args.refundReference) : null,
                args.lines as Array<{ source_line_id: string; quantity: string }>
              );
            case 'print_ticket':
              return api.print_ticket();
            default:
              throw new Error(`unexpected Tauri command: ${cmd}`);
          }
        },
        transformCallback: () => 1,
        unregisterCallback: () => undefined,
        convertFileSrc: (path: string) => path
      };
    });
  });

  test('login, add a product, take payment and print', async ({ page }) => {
    page.on('dialog', (dialog) => void dialog.accept('Devolución E2E'));
    await page.setViewportSize({ width: 1100, height: 700 });
    await page.goto('/');

    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');
    await page.getByRole('button', { name: 'Oscuro' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
    await page.getByRole('button', { name: 'Claro' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');

    await expect(page.getByRole('heading', { name: 'Identifícate' })).toBeVisible();
    await page.getByLabel('Usuario').fill('admin');
    await page.getByLabel('Contraseña').fill('admin123');
    await page.getByRole('button', { name: 'Entrar' }).click();

    await expect(page.getByRole('heading', { name: '100%SHOPPING' })).toBeVisible();

    await page.getByRole('button', { name: 'Sincronizar ahora' }).click();
    await expect(page.getByText(/Sincronizado: 1 aceptados, 1 usuario/)).toBeVisible();

    await page.getByRole('button', { name: 'Fichar', exact: true }).click();
    const timeClockDialog = page.getByRole('dialog', { name: 'Fichaje del personal' });
    await expect(timeClockDialog).toBeVisible();
    await expect(timeClockDialog.getByText('No tienes ningún fichaje abierto.')).toBeVisible();
    await timeClockDialog.getByRole('button', { name: 'Fichar entrada' }).click();
    await expect(timeClockDialog.getByRole('button', { name: 'Fichar salida' })).toBeVisible();
    await expect(timeClockDialog.getByText('En curso')).toBeVisible();
    await timeClockDialog.getByRole('button', { name: 'Fichar salida' }).click();
    await expect(timeClockDialog.getByRole('button', { name: 'Fichar entrada' })).toBeVisible();
    await expect(timeClockDialog.locator('.timeclock-row')).toHaveCount(1);
    await expect(timeClockDialog.locator('.timeclock-row')).toContainText('Administrador');
    await timeClockDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Notas', exact: true }).click();
    const notesDialog = page.getByRole('dialog', { name: 'Notas internas' });
    await expect(notesDialog).toBeVisible();
    await expect(notesDialog.getByText('No hay notas en la bandeja.')).toBeVisible();
    await notesDialog.getByLabel('Título').fill('Aviso E2E');
    await notesDialog.getByLabel('Contenido').fill('Revisar la caja antes de cerrar.');
    await notesDialog.getByRole('checkbox', { name: 'Administrador' }).check();
    await notesDialog.getByRole('button', { name: 'Publicar nota' }).click();
    await expect(notesDialog.locator('.note-card')).toHaveCount(1);
    await expect(notesDialog.locator('.note-card')).toContainText('Aviso E2E');
    await expect(notesDialog.locator('.note-card')).toContainText('Para: Administrador');
    await notesDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Clientes' }).click();
    const customers = page.getByRole('dialog', { name: 'Clientes' });
    await expect(customers).toBeVisible();
    await customers.getByLabel('Nombre').fill('Cliente');
    await customers.getByLabel('Primer apellido').fill('UI');
    await customers.getByLabel('NIF').fill('12345678Z');
    await customers.getByRole('button', { name: 'Guardar cliente' }).click();
    await expect(customers.locator('.customer-row', { hasText: 'Cliente UI' })).toBeVisible();
    await customers.getByRole('button', { name: 'Esc' }).click();
    await page.getByLabel('Cliente').selectOption({ label: 'Cliente UI · 12345678Z' });

    await page.getByRole('button', { name: 'Abrir caja · fondo 100,00 €' }).click();
    await expect(page.getByText('Caja abierta con fondo de 100,00 €.')).toBeVisible();
    await page.getByLabel('Importe del movimiento (€)').fill('3,50');
    await page.getByLabel('Motivo del movimiento').fill('Compra de hielo');
    await page.getByRole('button', { name: 'Registrar movimiento' }).click();
    await expect(page.getByText(/Movimiento registrado: Gasto 3.50 €/)).toBeVisible();
    const movementRow = page.locator('.movement-row', { hasText: 'Compra de hielo' });
    await expect(movementRow).toContainText('Gasto');
    await expect(movementRow).toContainText('−3.50 €');

    await page.getByRole('button', { name: 'Catálogo' }).click();
    await expect(page.getByRole('dialog', { name: 'Catálogo' })).toBeVisible();
    await expect(page.getByText('Sin conflictos de maestros.')).toBeVisible();

    await page.getByRole('tab', { name: 'Familias' }).click();
    await page.getByLabel('Código').fill('E2E');
    await page.getByLabel('Nombre').fill('Familia E2E');
    await page.getByRole('button', { name: 'Guardar familia' }).click();
    await expect(page.locator('.product-row', { hasText: 'Familia E2E' })).toBeVisible();

    await page.getByRole('tab', { name: 'Impuestos' }).click();
    await page.getByLabel('Código').fill('E2E5');
    await page.getByLabel('Nombre').fill('Especial E2E');
    await page.getByLabel('% IVA').fill('5');
    await page.getByRole('button', { name: 'Guardar impuesto' }).click();
    await expect(page.locator('.product-row', { hasText: 'Especial E2E' })).toContainText('5% IVA');

    await page.getByRole('tab', { name: 'Vendedores' }).click();
    await page.getByLabel('Código').fill('E2E-S');
    await page.getByLabel('Nombre').fill('Vendedor E2E');
    await page.getByLabel('Teléfono 1').fill('600111222');
    await page.getByRole('button', { name: 'Guardar vendedor' }).click();
    await expect(page.locator('.product-row', { hasText: 'Vendedor E2E' })).toContainText('600111222');

    await page.getByRole('tab', { name: 'Tiendas' }).click();
    await page.getByLabel('Código', { exact: true }).fill('E2E-STORE');
    await page.getByLabel('Nombre').fill('Tienda E2E');
    await page.getByLabel('NIF').fill('B12345674');
    await page.getByLabel('Calle').fill('Calle E2E');
    await page.getByLabel('Número').fill('7');
    await page.getByLabel('Localidad').fill('Benalmádena');
    await page.getByLabel('Código postal').fill('29630');
    await page.getByLabel('Provincia').fill('Málaga');
    await page.getByRole('button', { name: 'Guardar tienda' }).click();
    await expect(page.locator('.product-row', { hasText: 'Tienda E2E' })).toContainText('B12345674');

    await page.getByRole('tab', { name: 'Artículos' }).click();
    await page.getByLabel('SKU').fill('UI-E2E-1');
    await page.getByLabel('Nombre').fill('Producto E2E');
    await page.getByLabel('Familia').selectOption('E2E');
    await page.getByLabel('Código fiscal').selectOption('E2E5');
    await page.getByLabel('Precio', { exact: true }).fill('7.50');
    await page.getByRole('button', { name: 'Guardar producto' }).click();
    const productRow = page.locator('.product-row', { hasText: 'Producto E2E' });
    await expect(productRow).toBeVisible();
    await expect(productRow).toContainText('5% IVA');
    await productRow.getByRole('button', { name: 'Editar' }).click();
    await page.getByLabel('Nombre').fill('Producto E2E editado');
    await page.getByRole('button', { name: 'Guardar producto' }).click();
    await expect(page.locator('.product-row', { hasText: 'Producto E2E editado' })).toBeVisible();

    const catalogDialog = page.getByRole('dialog', { name: 'Catálogo' });
    await page.getByRole('tab', { name: 'Stock' }).click();
    await expect(page.locator('.stock-level-row.selected')).toContainText('Producto E2E editado');
    await catalogDialog.getByLabel('Cantidad', { exact: true }).fill('12');
    await catalogDialog.getByLabel('Motivo', { exact: true }).fill('Compra UI');
    await catalogDialog.getByRole('button', { name: 'Registrar movimiento' }).click();
    await expect(page.getByText(/Movimiento de stock: Entrada 12/)).toBeVisible();
    await expect(page.locator('.stock-current')).toContainText('12');
    await catalogDialog.getByLabel('Mínimo de stock').fill('20');
    await catalogDialog.getByRole('button', { name: 'Guardar mínimo' }).click();
    await expect(page.getByText(/Mínimo de stock actualizado: 20/)).toBeVisible();
    await expect(page.locator('.stock-level-row.selected')).toContainText('BAJO');
    await catalogDialog.getByLabel('Conteo físico').fill('7');
    await catalogDialog.getByLabel('Motivo del conteo').fill('Conteo UI');
    await expect(catalogDialog.locator('.stock-count-preview')).toContainText('Ajuste previsto');
    await expect(catalogDialog.locator('.stock-count-preview')).toContainText('-5');
    await catalogDialog.getByRole('button', { name: 'Ajustar a conteo' }).click();
    await expect(page.getByText(/Conteo físico ajustado: -5 · existencias 7/)).toBeVisible();
    await expect(page.locator('.stock-current')).toContainText('7');
    await expect(page.locator('.stock-current')).toContainText('Reponer 13');
    await expect(page.locator('.stock-level-row.selected')).toContainText('reponer 13');
    await catalogDialog.getByRole('button', { name: 'Preparar entrada' }).click();
    const stockDetail = catalogDialog.locator('.stock-detail-panel');
    await expect(stockDetail.getByLabel('Cantidad', { exact: true })).toHaveValue('13');
    await expect(stockDetail.getByLabel('Motivo', { exact: true })).toHaveValue(
      'Reposición sugerida'
    );
    await expect(stockDetail.getByLabel('Tipo de movimiento')).toHaveValue('purchase');
    await stockDetail.getByLabel('Cantidad', { exact: true }).fill('1');
    await stockDetail.getByLabel('Motivo', { exact: true }).fill('');
    await expect(page.locator('.stock-movements')).toContainText('Ajuste negativo');
    await expect(page.locator('.stock-movements')).toContainText('Conteo UI');

    await catalogDialog.getByRole('button', { name: 'Importar conteo' }).click();
    const countImportPanel = catalogDialog.locator('.stock-count-import');
    const countImportFile = countImportPanel.getByLabel('Fichero de conteo CSV');
    await countImportFile.setInputFiles({
      name: 'conteo-invalido.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from(
        'Codigo;Conteo fisico;Motivo\nUI-E2E-1;7;Importación UI\nNO-EXISTE;1;Error\n'
      )
    });
    await expect(countImportPanel).toContainText('1 con error');
    await expect(countImportPanel).toContainText('NO-EXISTE');
    await expect(countImportPanel.getByRole('button', { name: 'Confirmar conteo' })).toBeDisabled();
    await countImportFile.setInputFiles({
      name: 'conteo-valido.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from('Codigo;Conteo fisico;Motivo\nUI-E2E-1;7;Importación UI\n')
    });
    await expect(countImportPanel).toContainText('0 ajustes');
    await expect(countImportPanel).toContainText('1 sin cambios');
    await countImportPanel.getByRole('button', { name: 'Confirmar conteo' }).click();
    await expect(
      page.getByText(/Conteo importado · 0 ajustes · 1 sin cambios/)
    ).toBeVisible();
    await expect(countImportPanel).toContainText('CONTEO IMPORTADO');
    await catalogDialog.getByRole('button', { name: 'Cerrar importación' }).click();

    await catalogDialog.getByRole('button', { name: 'Exportar CSV' }).click();
    await expect(page.getByText(/Stock exportado · \/tmp\/stock-e2e\.csv/)).toBeVisible();
    await catalogDialog.getByRole('button', { name: 'Pedido de reposición' }).click();
    await expect(
      page.getByText(/Pedido de reposición exportado · \/tmp\/pedido-reposicion-e2e\.csv/)
    ).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastReplenishmentRequest?.status)
      )
      .toBe('restock');
    await expect(page.locator('.stock-movements')).toContainText('Compra UI');
    await catalogDialog.getByRole('button', { name: 'Exportar historial' }).click();
    await expect(
      page.getByText(/Historial exportado · \/tmp\/movimientos-e2e\.csv/)
    ).toBeVisible();
    const restockSummary = catalogDialog.locator('.stock-summary-card.restock');
    await expect(restockSummary.locator('strong')).toHaveText('1');
    await expect(restockSummary).toContainText('13 uds');
    await restockSummary.click();
    await expect(restockSummary).toHaveAttribute('aria-pressed', 'true');
    await expect(catalogDialog.locator('.stock-level-row')).toHaveCount(1);

    const lowSummary = catalogDialog.locator('.stock-summary-card.low');
    await expect(lowSummary.locator('strong')).toHaveText('1');
    await lowSummary.click();
    await expect(lowSummary).toHaveAttribute('aria-pressed', 'true');
    await expect(catalogDialog.locator('.stock-level-row')).toHaveCount(1);
    await catalogDialog.getByRole('button', { name: 'Exportar CSV' }).click();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastStockExportRequest?.status)
      )
      .toBe('low');
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastStockExportRequest?.search)
      )
      .toBeNull();
    await catalogDialog.getByRole('button', { name: 'Hoja de conteo' }).click();
    await expect(
      page.getByText(/Hoja de conteo exportada · \/tmp\/conteo-fisico-e2e\.csv/)
    ).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastCountSheetRequest?.status)
      )
      .toBe('low');

    const outSummary = catalogDialog.locator('.stock-summary-card.out');
    const outCount = Number(await outSummary.locator('strong').textContent());
    await outSummary.click();
    await expect(outSummary).toHaveAttribute('aria-pressed', 'true');
    await expect(catalogDialog.locator('.stock-level-row')).toHaveCount(outCount);

    const allSummary = catalogDialog.locator('.stock-summary-card.all');
    const allCount = Number(await allSummary.locator('strong').textContent());
    await allSummary.click();
    await expect(allSummary).toHaveAttribute('aria-pressed', 'true');
    await expect(catalogDialog.locator('.stock-level-row')).toHaveCount(allCount);
    await expect(catalogDialog).toBeVisible();

    await catalogDialog.getByRole('button', { name: 'Ver libro completo' }).click();
    await expect(page.getByRole('tab', { name: 'Movimientos' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await expect(catalogDialog.locator('.stock-ledger-filter')).toContainText('UI-E2E-1');
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(2);
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastLedgerRequest?.product_id)
      )
      .toBeTruthy();
    await catalogDialog.getByRole('button', { name: 'Quitar filtro' }).click();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastLedgerRequest?.product_id)
      )
      .toBeNull();
    await expect(catalogDialog.locator('.stock-ledger-filter')).toHaveCount(0);

    await page.getByRole('tab', { name: 'Movimientos' }).click();
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(2);
    await expect(catalogDialog.locator('.stock-ledger')).toContainText('UI-E2E-1');
    await catalogDialog
      .getByLabel('Tipo de movimiento del libro')
      .selectOption('purchase');
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(1);
    await expect(catalogDialog.locator('.stock-ledger-row')).toContainText('Entrada');
    await expect(catalogDialog.locator('.stock-ledger-row')).toContainText('Compra UI');
    await catalogDialog.getByLabel('Buscar movimientos').fill('compra ui');
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(1);
    await expect(catalogDialog.locator('.stock-ledger-row')).toContainText('Compra UI');
    await catalogDialog.getByLabel('Buscar movimientos').fill('sin-coincidencias');
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(
      catalogDialog.getByText('No hay movimientos que coincidan con los filtros.')
    ).toBeVisible();
    await catalogDialog.getByLabel('Buscar movimientos').fill('');
    await catalogDialog
      .getByLabel('Tipo de movimiento del libro')
      .selectOption('all');
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(2);
    const tomorrow = new Date(Date.now() + 86_400_000).toISOString().slice(0, 10);
    await catalogDialog.getByLabel('Desde el día').fill(tomorrow);
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(
      catalogDialog.getByText('No hay movimientos que coincidan con los filtros.')
    ).toBeVisible();
    await catalogDialog.getByLabel('Desde el día').fill('');
    await catalogDialog.getByRole('button', { name: 'Buscar', exact: true }).click();
    await expect(catalogDialog.locator('.stock-ledger-row')).toHaveCount(2);
    await catalogDialog.getByRole('button', { name: 'Exportar CSV' }).click();
    await expect(
      page.getByText(
        /Libro de movimientos exportado · \/tmp\/movimientos-stock-e2e\.csv/
      )
    ).toBeVisible();
    await catalogDialog
      .getByRole('button', { name: 'Ver stock de Producto E2E editado' })
      .first()
      .click();
    await expect(page.getByRole('tab', { name: 'Stock' })).toHaveAttribute(
      'aria-selected',
      'true'
    );
    await expect(catalogDialog.locator('.stock-detail-panel')).toContainText(
      'Producto E2E editado'
    );
    await expect(catalogDialog.locator('.stock-detail-panel')).toContainText('Reponer 13');

    await catalogDialog
      .getByLabel('Tienda destino')
      .selectOption({ label: 'E2E-STORE · Tienda E2E' });
    await catalogDialog.getByLabel('Cantidad a transferir').fill('1');
    await catalogDialog.getByLabel('Motivo de la transferencia').fill('Traslado UI');
    await catalogDialog.getByRole('button', { name: 'Transferir stock' }).click();
    await expect(
      page.getByText(/Transferencia enviada · 1 uds a Tienda E2E/)
    ).toBeVisible();
    await expect(catalogDialog.locator('.stock-current')).toContainText('6');
    await expect(catalogDialog.locator('.stock-movements')).toContainText(
      'Transferencia enviada'
    );
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastTransferRequest?.to_store_id)
      )
      .toBeTruthy();
    await expect
      .poll(() =>
        page.evaluate(() => (window as any).__lastTransferRequest?.quantity)
      )
      .toBe('1');

    await page.getByRole('tab', { name: 'Tarifas' }).click();
    await page.getByLabel('Código', { exact: true }).fill('E2E-PL');
    await page.getByLabel('Nombre').fill('Tarifa E2E');
    await page
      .locator('.price-list-item-form select')
      .selectOption({ label: 'UI-E2E-1 · Producto E2E editado' });
    await page.getByLabel('Precio', { exact: true }).fill('6,50');
    await page.getByRole('button', { name: 'Añadir precio' }).click();
    await expect(page.getByText('1 precio(s)')).toBeVisible();
    await expect(page.locator('.product-row', { hasText: '6.50 €' })).toBeVisible();
    await page.getByRole('button', { name: 'Guardar tarifa' }).click();
    const priceListRow = page.locator('.product-row', { hasText: 'Tarifa E2E' });
    await expect(priceListRow).toBeVisible();
    await expect(priceListRow).toContainText('E2E-PL');
    await expect(priceListRow).toContainText('ACTIVA');

    await page.getByRole('tab', { name: 'Artículos' }).click();
    await page.getByLabel('Fichero CSV').setInputFiles({
      name: 'productos.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from(
        'source_id,sku,name,unit_price,tax_percent_bp\nlegacy-1,IMP-1,Importado,2.00,2100\n'
      )
    });
    await expect(page.getByText('Lote e2e-batch')).toBeVisible();
    await expect(page.getByText('Simulación: todavía no se ha escrito nada.')).toBeVisible();
    await page.getByRole('button', { name: 'Confirmar importación' }).click();
    await expect(page.getByText('IMPORTACIÓN CONFIRMADA', { exact: true })).toBeVisible();
    await expect(
      page.getByText('Importación confirmada: 1 nuevos, 1 actualizados.')
    ).toBeVisible();
    await page.getByRole('dialog', { name: 'Catálogo' }).getByRole('button', { name: 'Esc' }).click();
    const lowStockButton = page.getByRole('button', { name: /Stock bajo/ });
    await expect(lowStockButton).toContainText('1');
    await lowStockButton.click();
    await expect(catalogDialog).toBeVisible();
    await expect(catalogDialog.locator('.stock-level-row')).toHaveCount(1);
    await catalogDialog.getByRole('button', { name: 'Esc' }).click();
    await page.getByRole('button', { name: 'Usuarios' }).click();
    const usersDialog = page.getByRole('dialog', { name: 'Gestión de usuarios' });
    await expect(usersDialog).toBeVisible();
    await expect(usersDialog.getByText('admin', { exact: true })).toBeVisible();
    const userFormOverflow = await usersDialog.evaluate((dialog) => {
      const form = dialog.querySelector('.user-create');
      if (!form) return true;
      const button = form.querySelector('button');
      if (!button) return true;
      const bounds = dialog.getBoundingClientRect();
      const buttonRect = button.getBoundingClientRect();
      return [...form.querySelectorAll('input, select')].some((element) => {
        const rect = element.getBoundingClientRect();
        return (
          rect.left < bounds.left ||
          rect.right > bounds.right ||
          !(
            rect.right <= buttonRect.left ||
            rect.left >= buttonRect.right ||
            rect.bottom <= buttonRect.top ||
            rect.top >= buttonRect.bottom
          )
        );
      });
    });
    expect(userFormOverflow).toBe(false);
    await usersDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByLabel('Tarifa').selectOption({ label: 'Tarifa E2E (E2E-PL)' });

    const search = page.getByRole('textbox', { name: 'Buscar artículo' });
    await search.fill('café');
    const searchResult = page.getByRole('button', { name: /3 · Café solo/ });
    await expect(searchResult.locator('.badge')).toContainText('5 uds');
    await expect(searchResult.locator('.badge')).toHaveClass(/warn/);
    await searchResult.click();
    await expect(page.getByText(/Línea añadida/)).toBeVisible();

    await page.getByRole('button', { name: /F5 · Cobrar/ }).click();
    const payment = page.getByRole('dialog', { name: 'Forma de pago' });
    await expect(payment).toBeVisible();
    await payment.getByLabel('Entregado por el cliente (céntimos)').fill('200');
    await payment.getByRole('button', { name: 'Aplicar pago' }).click();
    await expect(page.getByText(/Cobro completo/)).toBeVisible();

    await payment.getByRole('button', { name: 'Cobrar y cerrar' }).click();
    await expect(page.getByText(/Ticket 1 confirmado e impreso/)).toBeVisible();
    await expect(page.getByText('Cobrada').first()).toBeVisible();

    await page.getByRole('button', { name: 'Consultas', exact: true }).click();
    const historyDialog = page.getByRole('dialog', { name: 'Consultas de ventas' });
    await expect(historyDialog).toBeVisible();
    await expect(historyDialog.locator('.history-row')).toHaveCount(1);
    await historyDialog.locator('.history-row').click();
    await expect(historyDialog.getByRole('heading', { name: /Ticket #1/ })).toBeVisible();
    await expect(historyDialog.getByText('Café solo')).toBeVisible();
    await historyDialog.getByRole('button', { name: 'Volver' }).click();
    await historyDialog.getByLabel('Ticket').fill('1');
    await historyDialog.getByRole('button', { name: 'Buscar' }).click();
    await expect(historyDialog.locator('.history-row')).toHaveCount(1);
    await historyDialog.getByLabel('Ticket').fill('99');
    await historyDialog.getByRole('button', { name: 'Buscar' }).click();
    await expect(historyDialog.locator('.history-row')).toHaveCount(0);
    await expect(historyDialog.getByText('No hay ventas para esos filtros.')).toBeVisible();
    await historyDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Emitir factura' }).click();
    await expect(page.getByText(/Factura FAC-000001 emitida/)).toBeVisible();

    await page.getByRole('button', { name: 'Devolver' }).first().click();
    const refund = page.getByRole('dialog', { name: 'Devolución parcial' });
    await expect(refund).toBeVisible();
    await expect(refund.getByLabel('Referencia del reembolso')).toBeVisible();
    await refund.getByLabel('Cantidad a devolver Café solo').fill('0,5');
    await refund.getByLabel('Motivo de la devolución').fill('Devolución parcial E2E');
    await refund.getByRole('button', { name: 'Emitir devolución' }).click();
    await expect(page.getByText(/Devolución REC-000001 emitida/)).toBeVisible();
    await expect(page.locator('.movement-row', { hasText: 'Devolución REC-000001' })).toBeVisible();

    await page.getByRole('button', { name: 'Rectificar' }).first().click();
    await expect(page.getByText(/Rectificación REC-000001 emitida/)).toBeVisible();
  });
});
