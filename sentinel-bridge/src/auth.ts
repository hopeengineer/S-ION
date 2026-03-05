import { Request, Response, NextFunction } from "express";

/**
 * S-ION Auth: "Secret Handshake" middleware.
 * Only S-ION instances with the correct SION_BRIDGE_TOKEN can talk to this bridge.
 */
export function authMiddleware(req: Request, res: Response, next: NextFunction): void {
    const token = process.env.SION_BRIDGE_TOKEN;

    if (!token) {
        // No token configured: allow all (dev mode)
        console.warn("⚠️  SION_BRIDGE_TOKEN not set: running in open mode");
        next();
        return;
    }

    const provided = req.headers.authorization?.replace("Bearer ", "") || "";

    if (provided !== token) {
        res.status(401).json({ error: "Unauthorized: invalid SION_BRIDGE_TOKEN" });
        return;
    }

    next();
}
