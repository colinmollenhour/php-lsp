<?php

namespace App;

class Noise27
{
    private function compute(): int
    {
        return 27;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
