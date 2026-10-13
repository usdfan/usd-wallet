(function () {
    'use strict';

    if (window.__usdWalletLoaded) return;
    window.__usdWalletLoaded = true;

    var NETWORKS = ['TRC20', 'ERC20', 'Polygon'];
    var pending = new Set();
    var scheduled = false;
    var skipContent = '[contenteditable],script,style,textarea,.ls-post-content,.ls-ci-text,.ls-chat-msg-content-inner,.ls-comment-content,[data-usd-wallet-no-transform]';

    function isEditableOrContent(el) {
        return !!(el && el.closest && el.closest(skipContent));
    }

    function displayText(text, parent) {
        if (!text) return text;
        var value = text;
        value = value.replace(/预计到账按人民币展示/g, '预计到账按 USDT 展示');
        if (parent && parent.closest && parent.closest('[data-wd-preview],#lsw-wd-preview')) {
            value = value.replace(/预计到账\s*[¥￥$]\s*([\d,.]+)/g, '预计到账 $1 USDT');
        }
        value = value.replace(/[¥￥]/g, '$').replace(/人民币|RMB/gi, 'USD');
        value = value.replace(/(\d[\d,.]*\s*)元(?!素)/g, '$1 USD').replace(/（元）/g, '（USD）');
        return value;
    }

    function translate(root) {
        if (!root || !document.documentElement.contains(root)) return;
        if (root.nodeType === Node.TEXT_NODE) {
            if (!isEditableOrContent(root.parentElement)) {
                var next = displayText(root.nodeValue, root.parentElement);
                if (next !== root.nodeValue) root.nodeValue = next;
            }
            return;
        }
        if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE) return;
        if (root.nodeType === Node.ELEMENT_NODE && isEditableOrContent(root)) return;
        var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
            acceptNode: function (node) {
                return isEditableOrContent(node.parentElement) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
            }
        });
        while (walker.nextNode()) {
            var node = walker.currentNode;
            var value = displayText(node.nodeValue, node.parentElement);
            if (value !== node.nodeValue) node.nodeValue = value;
        }
        var selector = '[placeholder],[title],[aria-label]';
        var attrs = root.matches && root.matches(selector) ? [root] : [];
        if (root.querySelectorAll) attrs = attrs.concat(Array.from(root.querySelectorAll(selector)));
        attrs.forEach(function (el) {
            if (isEditableOrContent(el)) return;
            ['placeholder', 'title', 'aria-label'].forEach(function (attr) {
                if (!el.hasAttribute(attr)) return;
                var current = el.getAttribute(attr);
                var next = displayText(current, el);
                if (next !== current) el.setAttribute(attr, next);
            });
        });
        var icons = root.matches && root.matches('[class*="cny"]') ? [root] : [];
        if (root.querySelectorAll) icons = icons.concat(Array.from(root.querySelectorAll('[class*="cny"]')));
        icons.forEach(function (el) {
            Array.from(el.classList).forEach(function (name) {
                if (name.indexOf('ri-money-cny-') === 0 || name.indexOf('ri-exchange-cny-') === 0) {
                    el.classList.replace(name, name.replace('-cny-', '-dollar-'));
                }
            });
        });
    }

    function notifyError(message) {
        if (window.LS && LS.ui && LS.ui.toast && LS.ui.toast.error) LS.ui.toast.error(message);
        else window.alert(message);
    }

    function setNative(input, value) {
        if (!input || input.value === value) return;
        input.value = value;
        input.dispatchEvent(new Event('input', { bubbles: true }));
        input.dispatchEvent(new Event('change', { bubbles: true }));
    }

    function fieldLabel(input) {
        var field = input.closest('.lsw-wd-field');
        if (field) return field.querySelector('label');
        var previous = input.previousElementSibling;
        return previous && previous.tagName === 'LABEL' ? previous : null;
    }

    function keepNativeFieldHidden(input, form) {
        var field = input.closest('.lsw-wd-field');
        var label = fieldLabel(input);
        input.type = 'hidden';
        input.setAttribute('aria-hidden', 'true');
        form.appendChild(input);
        if (field) field.remove();
        else if (label) label.remove();
    }

    function selectedNetwork(extra) {
        return NETWORKS.indexOf(extra) >= 0 ? extra : 'TRC20';
    }

    function networkField(mobile, network) {
        var field = document.createElement('div');
        field.className = 'usd-wallet-network' + (mobile ? ' usd-wallet-network-mobile' : ' lsw-wd-field');
        var label = document.createElement('label');
        label.textContent = '选择提现网络';
        if (mobile) label.className = 'ls-wallet-label';
        var select = document.createElement('select');
        select.className = mobile ? 'ls-wallet-input' : '';
        select.setAttribute('data-usd-wallet-network', '');
        select.setAttribute('aria-label', '选择提现网络');
        NETWORKS.forEach(function (name) {
            var option = document.createElement('option');
            option.value = name;
            option.textContent = name;
            select.appendChild(option);
        });
        select.value = network;
        field.appendChild(label);
        field.appendChild(select);
        return field;
    }

    function formForSubmit(submit) {
        var mobileForm = submit.closest('.ls-wallet-form');
        if (mobileForm) return mobileForm;
        var pcBody = submit.closest('#lsw-body');
        return pcBody && pcBody.querySelector('.lsw-wd-form');
    }

    function decorateForm(submit) {
        var form = formForSubmit(submit);
        if (!form) return;
        var mobile = form.classList.contains('ls-wallet-form');
        var bank = form.querySelector(mobile ? '[data-wd-method="bank"]' : '[data-wm="bank"]');
        var others = form.querySelectorAll(mobile ? '[data-wd-method]:not([data-wd-method="bank"])' : '[data-wm]:not([data-wm="bank"])');
        others.forEach(function (node) { node.remove(); });
        if (!bank) {
            submit.disabled = true;
            if (!form.querySelector('.usd-wallet-unavailable')) {
                var warning = document.createElement('p');
                warning.className = 'usd-wallet-unavailable';
                warning.textContent = 'USDT 提现未启用：请管理员在原生提现设置中开启 bank 渠道。';
                submit.parentNode.insertBefore(warning, submit);
            }
            return;
        }
        if (!(bank.classList.contains('on') || bank.classList.contains('is-active'))) {
            bank.click();
            return;
        }
        if (form.hasAttribute('data-usd-wallet-ready')) return;

        var nameInput = form.querySelector('[data-wd-account-name]');
        var addressInput = form.querySelector('[data-wd-account-no]');
        var extraInput = form.querySelector('[data-wd-account-extra]');
        if (!nameInput || !addressInput || !extraInput) return;
        form.setAttribute('data-usd-wallet-ready', '');

        var amountInput = form.querySelector('[data-wd-amt],[data-wd-amount]');
        var amountLabel = amountInput && fieldLabel(amountInput);
        if (amountLabel) amountLabel.textContent = '提现金额 ($)';

        setNative(nameInput, 'USDT');
        var network = selectedNetwork(extraInput.value);
        setNative(extraInput, network);

        var addressLabel = fieldLabel(addressInput);
        if (addressLabel) addressLabel.textContent = 'USDT 钱包地址';
        addressInput.placeholder = '输入所选网络的 USDT 钱包地址';
        addressInput.setAttribute('autocomplete', 'off');
        addressInput.setAttribute('spellcheck', 'false');

        var field = networkField(mobile, network);
        if (mobile) {
            form.insertBefore(field, addressLabel || addressInput);
        } else {
            var grid = addressInput.closest('.lsw-wd-grid');
            var addressField = addressInput.closest('.lsw-wd-field');
            if (grid && addressField) {
                form.insertBefore(field, grid);
                form.insertBefore(addressField, grid);
                grid.remove();
            } else {
                form.insertBefore(field, addressField || addressInput);
            }
        }
        keepNativeFieldHidden(nameInput, form);
        keepNativeFieldHidden(extraInput, form);
        field.querySelector('select').addEventListener('change', function (event) {
            setNative(extraInput, event.target.value);
            setNative(addressInput, '');
            addressInput.focus();
        });
    }

    function decorateForms() {
        document.querySelectorAll('[data-wd-submit]').forEach(decorateForm);
    }

    function validFormat(network, address) {
        if (network === 'TRC20') return /^T[1-9A-HJ-NP-Za-km-z]{33}$/.test(address);
        if (network === 'ERC20' || network === 'Polygon') return /^0x[0-9a-fA-F]{40}$/.test(address);
        return false;
    }

    document.addEventListener('click', function (event) {
        var submit = event.target.closest && event.target.closest('[data-wd-submit]');
        if (!submit) return;
        var form = formForSubmit(submit);
        if (!form) return;
        var networkSelect = form.querySelector('[data-usd-wallet-network]');
        var addressInput = form.querySelector('[data-wd-account-no]');
        var bank = form.querySelector('[data-wm="bank"],[data-wd-method="bank"]');
        var network = networkSelect && networkSelect.value;
        var address = addressInput && addressInput.value.trim();
        if (!bank || !network || !validFormat(network, address)) {
            event.preventDefault();
            event.stopImmediatePropagation();
            notifyError('请检查所选网络和钱包地址；TRC20 使用 T 开头地址，ERC20/Polygon 使用 0x 开头地址。');
        }
    }, true);

    function flush() {
        scheduled = false;
        pending.forEach(translate);
        pending.clear();
        decorateForms();
    }

    function schedule(node) {
        pending.add(node);
        if (!scheduled) {
            scheduled = true;
            requestAnimationFrame(flush);
        }
    }

    var observer = new MutationObserver(function (records) {
        records.forEach(function (record) {
            if (record.type === 'characterData') schedule(record.target);
            if (record.type === 'attributes') schedule(record.target);
            if (record.type === 'childList') {
                record.addedNodes.forEach(schedule);
                if (record.target.closest && record.target.closest('.lsw-wd-form,.ls-wallet-form')) schedule(record.target);
            }
        });
    });
    observer.observe(document.documentElement, {
        subtree: true,
        childList: true,
        characterData: true,
        attributes: true,
        attributeFilter: ['placeholder', 'title', 'aria-label', 'class']
    });
    schedule(document.documentElement);
})();
