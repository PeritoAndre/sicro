-- SICRO 4.0 — Nome do caso (migration 022).
--
-- O Início passou a pedir UMA coisa para criar um caso: um nome livre
-- ("Laudo 63404/26 — Km 09 Duca Serra"). BO, protocolo, tipo e município
-- continuam existindo, mas viraram dados opcionais (menu ⋯ → Dados do caso).
-- NULL em casos antigos: o rótulo cai no "BO — tipo — município" de sempre.

ALTER TABLE occurrences ADD COLUMN titulo TEXT;
