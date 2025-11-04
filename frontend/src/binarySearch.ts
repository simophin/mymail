
export function binarySearchBy<T, N>(
    haystack: {
        [key: number]: T;
        length: number;
    },
    needle: N,
    accessor: (item: T) => N,
    startIndex: number = 0,
    endIndex: number = haystack.length,
): number {
    let low = startIndex;
    let high = endIndex;

    while (low < high) {
        const mid = Math.floor((low + high) / 2);
        const midValue = accessor(haystack[mid]);

        if (midValue < needle) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    return low;
}