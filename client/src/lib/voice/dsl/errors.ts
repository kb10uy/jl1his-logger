export class DslError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'DslError';
    }
}
