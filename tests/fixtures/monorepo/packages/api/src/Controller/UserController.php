<?php
namespace Acme\Api\Controller;

use Acme\Core\Entity\User;
use Acme\Core\Repository\UserRepository;

class UserController {
    public function __construct(
        private UserRepository $repository,
    ) {}

    public function show(int $id): ?User {
        return $this->repository->findById($id);
    }

    public function index(): array {
        return $this->repository->findAll();
    }
}
