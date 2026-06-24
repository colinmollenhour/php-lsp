<?php

namespace App;

class Stranger
{
    protected function boot(): void
    {
    }

    public function go(): void
    {
        $this->boot();
    }
}
