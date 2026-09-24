    // HTML date/time input algorithms use UTC even when the user's local
    // timezone differs. datetime-local deliberately does not expose
    // valueAsDate because a timezone-free instant is ambiguous.
    // https://html.spec.whatwg.org/multipage/input.html#dom-input-valueasdate
    const inputDateTypes = new Set(['date', 'month', 'week', 'time']);
    const inputNumericDateTypes = new Set([...inputDateTypes, 'datetime-local']);
    const twoDigits = value => String(value).padStart(2, '0');
    const inputYear = value => String(value).padStart(4, '0');
    const utcDate = (year, month, day) => {
        const value = new Date(0);
        // Date.UTC treats years 0..99 as 1900..1999; setUTCFullYear does not.
        value.setUTCFullYear(year, month - 1, day);
        value.setUTCHours(0, 0, 0, 0);
        return value;
    };
    const isoWeekMonday = (year, week) => {
        const januaryFourth = utcDate(year, 1, 4);
        const weekday = januaryFourth.getUTCDay() || 7;
        januaryFourth.setUTCDate(4 - weekday + 1 + (week - 1) * 7);
        return januaryFourth;
    };
    const isoWeekOf = date => {
        const thursday = utcDate(date.getUTCFullYear(), date.getUTCMonth() + 1, date.getUTCDate());
        thursday.setUTCDate(thursday.getUTCDate() + 4 - (thursday.getUTCDay() || 7));
        const year = thursday.getUTCFullYear();
        const firstThursday = isoWeekMonday(year, 1);
        return [year, 1 + Math.floor((thursday - firstThursday) / 604800000)];
    };
    const inputDateFromValue = (type, value) => {
        if (!value || !inputDateTypes.has(type)) return null;
        let date;
        if (type === 'date') {
            const parts = /^([0-9]{4,})-([0-9]{2})-([0-9]{2})$/.exec(value);
            if (!parts) return null;
            date = utcDate(Number(parts[1]), Number(parts[2]), Number(parts[3]));
        } else if (type === 'month') {
            const parts = /^([0-9]{4,})-([0-9]{2})$/.exec(value);
            if (!parts) return null;
            date = utcDate(Number(parts[1]), Number(parts[2]), 1);
        } else if (type === 'week') {
            const parts = /^([0-9]{4,})-W([0-9]{2})$/.exec(value);
            if (!parts) return null;
            date = isoWeekMonday(Number(parts[1]), Number(parts[2]));
        } else {
            const parts = /^([0-9]{2}):([0-9]{2})(?::([0-9]{2})(?:\.([0-9]{1,3}))?)?$/.exec(value);
            if (!parts) return null;
            date = new Date(0);
            date.setUTCHours(Number(parts[1]), Number(parts[2]),
                Number(parts[3] || 0), Number((parts[4] || '').padEnd(3, '0')));
        }
        return Number.isFinite(date.getTime()) ? date : null;
    };
    const inputValueFromDate = (type, value) => {
        const year = value.getUTCFullYear();
        if (year < 1 && type !== 'time') return '';
        if (type === 'date')
            return inputYear(year) + '-' + twoDigits(value.getUTCMonth() + 1) + '-' +
                twoDigits(value.getUTCDate());
        if (type === 'month')
            return inputYear(year) + '-' + twoDigits(value.getUTCMonth() + 1);
        if (type === 'week') {
            const [weekYear, week] = isoWeekOf(value);
            return inputYear(weekYear) + '-W' + twoDigits(week);
        }
        let text = twoDigits(value.getUTCHours()) + ':' + twoDigits(value.getUTCMinutes());
        if (value.getUTCSeconds() || value.getUTCMilliseconds()) {
            text += ':' + twoDigits(value.getUTCSeconds());
            if (value.getUTCMilliseconds())
                text += '.' + String(value.getUTCMilliseconds()).padStart(3, '0').replace(/0+$/, '');
        }
        return text;
    };
    const inputNumberFromValue = (type, value) => {
        if (!inputNumericDateTypes.has(type) || !value) return NaN;
        if (type === 'month') {
            const date = inputDateFromValue(type, value);
            return date ? (date.getUTCFullYear() - 1970) * 12 + date.getUTCMonth() : NaN;
        }
        if (type === 'datetime-local') {
            const parts = /^([0-9]{4,})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2})(?::([0-9]{2})(?:\.([0-9]{1,3}))?)?$/.exec(value);
            if (!parts) return NaN;
            const date = utcDate(Number(parts[1]), Number(parts[2]), Number(parts[3]));
            date.setUTCHours(Number(parts[4]), Number(parts[5]),
                Number(parts[6] || 0), Number((parts[7] || '').padEnd(3, '0')));
            return date.getTime();
        }
        return inputDateFromValue(type, value)?.getTime() ?? NaN;
    };
    const inputValueFromNumber = (type, number) => {
        if (!Number.isFinite(number)) return '';
        if (type === 'month') {
            const months = Math.trunc(number);
            const year = 1970 + Math.floor(months / 12);
            return year < 1 ? '' : inputYear(year) + '-' +
                twoDigits(((months % 12) + 12) % 12 + 1);
        }
        const millis = type === 'time'
            ? ((number % 86400000) + 86400000) % 86400000 : number;
        const date = new Date(millis);
        if (!Number.isFinite(date.getTime())) return '';
        if (type === 'datetime-local')
            return inputValueFromDate('date', date) + 'T' +
                inputValueFromDate('time', date);
        return inputValueFromDate(type, date);
    };
